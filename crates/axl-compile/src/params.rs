//! Physical constants from `assets/physics/hardware.ron` (all `placeholder: calibrate`).

use serde::{Deserialize, Serialize};

use crate::error::CompileError;

/// DC motor parameters at the output shaft (SPEC.md, "Motor parameters").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotorParams {
    /// Nominal voltage, V.
    pub v_nom: f64,
    /// Largest voltage the battery can apply (full pack), V. Sets `ctrlrange`.
    pub v_max: f64,
    /// No-load speed at `v_nom`, rad/s.
    pub omega0: f64,
    /// Stall torque at `v_nom`, N m.
    pub stall_torque: f64,
    /// Winding resistance, ohm.
    pub resistance: f64,
    /// Reflected rotor inertia, kg m2.
    pub armature: f64,
    /// Gearbox friction loss, N m.
    pub friction_loss: f64,
}

/// The three motors.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotorTable {
    /// Medium motor.
    pub medium: MotorParams,
    /// Large motor.
    pub large: MotorParams,
    /// 180 degree servo.
    pub servo: MotorParams,
}

/// Contact solver and mass limits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolverParams {
    /// `solref` of every geom.
    pub solref: (f64, f64),
    /// `solimp` of every geom.
    pub solimp: (f64, f64, f64),
    /// No body is lighter than this, kg.
    pub min_body_mass_kg: f64,
}

/// Sliding friction values.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrictionTable {
    /// ABS plastic.
    pub abs: f64,
    /// Rubber tyre, sliding.
    pub rubber_sliding: f64,
    /// Rubber tyre, torsional.
    pub rubber_torsional: f64,
    /// Steel.
    pub steel: f64,
    /// Floor mat.
    pub floor: f64,
    /// Arena walls.
    pub wall: f64,
}

/// Damping and friction loss of one joint kind.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointParams {
    /// N m s/rad.
    pub damping: f64,
    /// N m.
    pub friction_loss: f64,
}

/// The joint table.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointTable {
    /// Smooth pin in a round hole.
    pub smooth_pin: JointParams,
    /// Friction pin.
    pub friction_pin: JointParams,
    /// Axle in a round hole.
    pub axle_in_hole: JointParams,
    /// Damping of a motor's own hinge.
    pub motor_damping: f64,
    /// Added to the driven joint of every gear mesh.
    pub gear_mesh_friction_loss: f64,
    /// Damping of the caster's ball joint.
    pub caster_ball_damping: f64,
}

/// Everything in `hardware.ron` that the compiler needs.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsParams {
    /// File format version.
    pub version: u32,
    /// Solver values.
    pub solver: SolverParams,
    /// Motors.
    pub motors: MotorTable,
    /// Friction.
    pub friction: FrictionTable,
    /// Joints.
    pub joints: JointTable,
}

impl PhysicsParams {
    /// The file shipped with the program.
    pub fn builtin() -> Self {
        let text = include_str!("../../../assets/physics/hardware.ron");
        // The file is part of the source tree and a test parses it, so this cannot fail.
        Self::parse(text).unwrap_or_else(|e| panic!("assets/physics/hardware.ron: {e}"))
    }

    /// Parse the RON text.
    pub fn parse(text: &str) -> Result<Self, CompileError> {
        ron::from_str(text).map_err(|e| CompileError::Params(e.to_string()))
    }
}
