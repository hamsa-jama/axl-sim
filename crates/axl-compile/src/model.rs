//! The `RobotModel`: the assembly after merging, as a tree of rigid bodies joined by joints, with
//! loop closures and gear couplings. Everything is in the robot frame or a body frame, in metres.
//! Body frames have the same orientation as the robot frame (every joint starts at angle 0).

use axl_build::Port;
use axl_parts::types::DeviceModel;

use crate::params::MotorParams;

/// Surface kind of a geom, which picks its friction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// ABS plastic.
    Abs,
    /// Rubber tyre.
    Rubber,
    /// Steel.
    Steel,
}

/// A collision shape in a body frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeomShape {
    /// Axis-aligned box.
    Box {
        /// Centre.
        pos: [f64; 3],
        /// Half sizes.
        half: [f64; 3],
    },
    /// Cylinder between two end-cap centres.
    Cylinder {
        /// One end.
        from: [f64; 3],
        /// The other end.
        to: [f64; 3],
        /// Radius.
        radius: f64,
    },
    /// Sphere.
    Sphere {
        /// Centre.
        pos: [f64; 3],
        /// Radius.
        radius: f64,
    },
}

/// One geom of a body.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelGeom {
    /// Name, `p<uid>_<sub>_<k>`.
    pub name: String,
    /// Part uid it came from.
    pub uid: u32,
    /// Shape in the body frame.
    pub shape: GeomShape,
    /// Mass, kg (the sub-body's mass split by shape volume).
    pub mass: f64,
    /// Surface.
    pub surface: Surface,
}

/// A part (or sub-body of a part) held by a body, for the renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPart {
    /// Part uid.
    pub uid: u32,
    /// Sub-body index in the part definition (0 for most parts, 1 for a motor's rotor).
    pub sub: u8,
    /// Part-frame origin in the body frame, metres.
    pub pos: [f64; 3],
    /// Orientation index 0..=23 (see `axl_build::Orient`).
    pub orient: u8,
}

/// How a body joins its parent.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelJoint {
    /// A hinge.
    Hinge {
        /// Name.
        name: String,
        /// Unit axis in the robot frame.
        axis: [f64; 3],
        /// Damping, N m s/rad.
        damping: f64,
        /// Friction loss, N m.
        friction_loss: f64,
        /// Reflected rotor inertia, kg m2.
        armature: f64,
        /// Travel limits, radians.
        range: Option<[f64; 2]>,
        /// Uid of the motor whose hinge this is.
        motor: Option<u32>,
    },
    /// A ball joint (the caster).
    Ball {
        /// Name.
        name: String,
        /// Damping, N m s/rad.
        damping: f64,
    },
}

impl ModelJoint {
    /// The joint's name.
    pub fn name(&self) -> &str {
        match self {
            ModelJoint::Hinge { name, .. } | ModelJoint::Ball { name, .. } => name,
        }
    }
}

/// A site (sensor frame).
#[derive(Clone, Debug, PartialEq)]
pub struct ModelSite {
    /// Name.
    pub name: String,
    /// Position in the body frame.
    pub pos: [f64; 3],
    /// Viewing axis (z axis of the site), or `None` for the robot frame.
    pub zaxis: Option<[f64; 3]>,
}

/// One MuJoCo body: a cluster of rigidly connected parts.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelBody {
    /// Name (`root` for the root).
    pub name: String,
    /// Index of the parent body in [`RobotModel::bodies`].
    pub parent: Option<usize>,
    /// Origin in the robot frame (the joint anchor; the hub for the root).
    pub origin: [f64; 3],
    /// The joint to the parent.
    pub joint: Option<ModelJoint>,
    /// Parts held.
    pub parts: Vec<BodyPart>,
    /// Collision shapes.
    pub geoms: Vec<ModelGeom>,
    /// Sites.
    pub sites: Vec<ModelSite>,
}

/// A loop closure: a joint that is not in the spanning tree becomes two `connect` constraints.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopClosure {
    /// Name prefix.
    pub name: String,
    /// First body (index in [`RobotModel::bodies`]).
    pub body1: usize,
    /// Second body.
    pub body2: usize,
    /// Anchor point, robot frame.
    pub anchor: [f64; 3],
    /// Unit axis, robot frame.
    pub axis: [f64; 3],
}

/// A gear coupling `q1 = ratio * q2`.
#[derive(Clone, Debug, PartialEq)]
pub struct GearCouple {
    /// Name.
    pub name: String,
    /// Joint of the first gear.
    pub joint1: String,
    /// Joint of the second gear.
    pub joint2: String,
    /// The polynomial coefficient `r` (SPEC: `-teeth2 / teeth1`, signed for joint axes).
    pub ratio: f64,
}

/// One device (motor, sensor, hub) of the robot.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDevice {
    /// Name given in the builder.
    pub name: String,
    /// Part uid.
    pub uid: u32,
    /// Hub port; `None` only for the hub.
    pub port: Option<Port>,
    /// Which device.
    pub kind: DeviceModel,
    /// Motor hinge joint name.
    pub joint: Option<String>,
    /// Motor parameters.
    pub motor: Option<MotorParams>,
    /// Sensor site name.
    pub site: Option<String>,
}

/// The merged robot.
#[derive(Clone, Debug, PartialEq)]
pub struct RobotModel {
    /// Robot name.
    pub name: String,
    /// Bodies, parents before children (breadth-first order); index 0 is the root.
    pub bodies: Vec<ModelBody>,
    /// Loop closures.
    pub loops: Vec<LoopClosure>,
    /// Gear couplings.
    pub gears: Vec<GearCouple>,
    /// Devices in uid order.
    pub devices: Vec<ModelDevice>,
    /// Lowest collision point in the robot frame, metres.
    pub min_z: f64,
    /// Things the compiler adjusted or skipped (for example added mass).
    pub warnings: Vec<String>,
}
