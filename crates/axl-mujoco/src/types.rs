//! Plain data types of the physics API.

/// Index of a body in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId(pub usize);
/// Index of a joint in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JointId(pub usize);
/// Index of an actuator in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActuatorId(pub usize);
/// Index of a sensor in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SensorId(pub usize);
/// Index of a geom in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeomId(pub usize);
/// Index of a site in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteId(pub usize);

/// A position and orientation in the world frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Position in metres.
    pub pos: [f64; 3],
    /// Unit quaternion `[w, x, y, z]` (MuJoCo order).
    pub quat: [f64; 4],
}

/// Which MuJoCo geom groups (0 to 5) a ray can hit. Bit `n` is group `n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupMask(pub u8);

impl GroupMask {
    /// Groups 0 (static world) and 2 (loose objects): what robot sensors see.
    /// Group 1 is the robot itself, so a robot never sees its own parts.
    pub const WORLD_AND_OBJECTS: GroupMask = GroupMask(0b0000_0101);
    /// Every group.
    pub const ALL: GroupMask = GroupMask(0b0011_1111);

    /// The `geomgroup` array MuJoCo expects.
    pub(crate) fn to_array(self) -> [u8; 6] {
        std::array::from_fn(|g| u8::from(self.0 & (1 << g) != 0))
    }
}

/// Where a ray hit a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// The geom that was hit.
    pub geom: GeomId,
    /// Distance along the ray in metres.
    pub distance: f64,
    /// The hit point in the world frame.
    pub point: [f64; 3],
    /// Unit surface normal at the hit point, pointing out of the surface.
    pub normal: [f64; 3],
}

/// One contact between two geoms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactInfo {
    /// Contact position in the world frame.
    pub pos: [f64; 3],
    /// Contact normal in the world frame, pointing from `geom1` to `geom2`.
    pub normal: [f64; 3],
    /// Force along the normal in newtons (never negative).
    pub normal_force: f64,
    /// Friction forces along the two tangent directions, in newtons.
    pub friction_force: [f64; 2],
    /// Signed distance between the surfaces in metres (negative = penetration).
    pub dist: f64,
    /// First geom of the pair.
    pub geom1: GeomId,
    /// Second geom of the pair.
    pub geom2: GeomId,
}

/// Counters of MuJoCo's warnings since the last reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsWarnings {
    /// A joint position was NaN or too large.
    pub bad_qpos: u32,
    /// A joint velocity was NaN or too large.
    pub bad_qvel: u32,
    /// A joint acceleration was NaN or too large.
    pub bad_qacc: u32,
    /// A control value was NaN or infinite.
    pub bad_ctrl: u32,
    /// Too many contacts for the preallocated buffer.
    pub contact_full: u32,
    /// Too many constraints for the preallocated buffer.
    pub constraint_full: u32,
    /// A body has a bad inertia.
    pub inertia: u32,
}

impl PhysicsWarnings {
    /// True if the simulation has become numerically unstable
    /// (NaN or bad position, velocity, acceleration or control).
    pub fn is_unstable(&self) -> bool {
        self.bad_qpos + self.bad_qvel + self.bad_qacc + self.bad_ctrl > 0
    }
}

/// A saved copy of the full integration state, for exact restore.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsState {
    pub(crate) values: Vec<f64>,
}

impl PhysicsState {
    /// The raw values, for hashing.
    pub fn as_slice(&self) -> &[f64] {
        &self.values
    }
}
