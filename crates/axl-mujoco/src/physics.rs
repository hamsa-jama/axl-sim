//! The [`Physics`] type: one MuJoCo model plus its data.

use mujoco_rs::prelude::*;

use crate::error::PhysicsError;
use crate::types::{
    ActuatorId, BodyId, ContactInfo, GeomId, GroupMask, JointId, PhysicsState, PhysicsWarnings,
    Pose, RayHit, SensorId, SiteId,
};

/// Which parts of `mjData` a saved state holds: everything that influences
/// the next step, so a restored run continues bit for bit.
const STATE_SPEC: u32 = MjtState::mjSTATE_INTEGRATION as u32;

/// A MuJoCo model with its data. Create with [`Physics::from_mjcf`].
pub struct Physics {
    data: MjData<Box<MjModel>>,
}

impl std::fmt::Debug for Physics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Physics")
            .field("time", &self.time())
            .field("nbody", &self.body_count())
            .finish()
    }
}

impl Physics {
    /// Compile MJCF text and create the simulation data, with the clock at 0.
    pub fn from_mjcf(xml: &str) -> Result<Self, PhysicsError> {
        let model = MjModel::from_xml_string(xml).map_err(|e| PhysicsError::Load(e.to_string()))?;
        let mut data = MjData::new(Box::new(model));
        // Compute positions and sensors for the initial pose.
        data.forward();
        Ok(Self { data })
    }

    /// Advance exactly one `mj_step` (one model timestep), then refresh body,
    /// geom and site poses so they match the new state.
    ///
    /// Contacts, sensor values and body velocities still describe the start of
    /// the step (MuJoCo computes them before integrating); call
    /// [`Physics::forward`] when you need them current.
    pub fn step(&mut self) {
        self.data.step();
        self.data.forward_kinematics();
    }

    /// Simulation time in seconds.
    pub fn time(&self) -> f64 {
        self.data.time()
    }

    /// The model's timestep in seconds.
    pub fn timestep(&self) -> f64 {
        self.data.model().opt().timestep
    }

    /// Reset to the initial state (time 0, all velocities zero) and recompute.
    pub fn reset(&mut self) {
        self.data.reset();
        self.data.forward();
    }

    /// Recompute positions, sensors and forces without advancing time.
    /// Call after changing the state by hand.
    pub fn forward(&mut self) {
        self.data.forward();
    }

    // ----- counts and names -------------------------------------------------

    /// Number of bodies, including the world body (id 0).
    pub fn body_count(&self) -> usize {
        self.data.model().ffi().nbody as usize
    }

    /// Number of geoms.
    pub fn geom_count(&self) -> usize {
        self.data.model().ffi().ngeom as usize
    }

    /// Number of joints.
    pub fn joint_count(&self) -> usize {
        self.data.model().ffi().njnt as usize
    }

    /// Number of actuators.
    pub fn actuator_count(&self) -> usize {
        self.data.model().ffi().nu as usize
    }

    /// Number of sensors.
    pub fn sensor_count(&self) -> usize {
        self.data.model().ffi().nsensor as usize
    }

    fn lookup(&self, kind: MjtObj, label: &'static str, name: &str) -> Result<usize, PhysicsError> {
        self.data
            .model()
            .name_to_id(kind, name)
            .ok_or_else(|| PhysicsError::UnknownName {
                kind: label,
                name: name.to_owned(),
            })
    }

    /// Find a body by its MJCF name.
    pub fn body_id(&self, name: &str) -> Result<BodyId, PhysicsError> {
        self.lookup(MjtObj::mjOBJ_BODY, "body", name).map(BodyId)
    }

    /// Find a joint by its MJCF name.
    pub fn joint_id(&self, name: &str) -> Result<JointId, PhysicsError> {
        self.lookup(MjtObj::mjOBJ_JOINT, "joint", name).map(JointId)
    }

    /// Find an actuator by its MJCF name.
    pub fn actuator_id(&self, name: &str) -> Result<ActuatorId, PhysicsError> {
        self.lookup(MjtObj::mjOBJ_ACTUATOR, "actuator", name)
            .map(ActuatorId)
    }

    /// Find a sensor by its MJCF name.
    pub fn sensor_id(&self, name: &str) -> Result<SensorId, PhysicsError> {
        self.lookup(MjtObj::mjOBJ_SENSOR, "sensor", name)
            .map(SensorId)
    }

    /// Find a geom by its MJCF name.
    pub fn geom_id(&self, name: &str) -> Result<GeomId, PhysicsError> {
        self.lookup(MjtObj::mjOBJ_GEOM, "geom", name).map(GeomId)
    }

    /// Find a site by its MJCF name.
    pub fn site_id(&self, name: &str) -> Result<SiteId, PhysicsError> {
        self.lookup(MjtObj::mjOBJ_SITE, "site", name).map(SiteId)
    }

    // ----- controls and readings -------------------------------------------

    /// Set an actuator's control input (volts for AXL motors).
    /// Out-of-range ids are ignored.
    pub fn set_ctrl(&mut self, a: ActuatorId, volts: f64) {
        if let Some(c) = self.data.ctrl_mut().get_mut(a.0) {
            *c = volts;
        }
    }

    /// Joint position in radians (metres for slide joints). Zero for a bad id
    /// and for free joints, which have no single angle.
    pub fn joint_pos(&self, j: JointId) -> f64 {
        let m = self.data.model();
        match m.jnt_qposadr().get(j.0) {
            Some(&adr) => self.data.qpos().get(adr as usize).copied().unwrap_or(0.0),
            None => 0.0,
        }
    }

    /// Joint velocity in rad/s. Zero for a bad id.
    pub fn joint_vel(&self, j: JointId) -> f64 {
        let m = self.data.model();
        match m.jnt_dofadr().get(j.0) {
            Some(&adr) => self.data.qvel().get(adr as usize).copied().unwrap_or(0.0),
            None => 0.0,
        }
    }

    /// Overwrite a hinge or slide joint's position (and zero its velocity).
    /// Call [`Physics::forward`] afterwards to refresh derived values.
    pub fn set_joint_pos(&mut self, j: JointId, value: f64) {
        let adr = self.data.model().jnt_qposadr().get(j.0).copied();
        if let Some(adr) = adr
            && let Some(q) = self.data.qpos_mut().get_mut(adr as usize)
        {
            *q = value;
        }
    }

    /// Pose of a body frame in the world. The world body (id 0) is the identity.
    pub fn body_pose(&self, b: BodyId) -> Pose {
        let pos = self.data.xpos().get(b.0).copied().unwrap_or([0.0; 3]);
        let quat = self
            .data
            .xquat()
            .get(b.0)
            .copied()
            .unwrap_or([1.0, 0.0, 0.0, 0.0]);
        Pose { pos, quat }
    }

    /// World position of a body's centre of mass.
    pub fn body_com(&self, b: BodyId) -> [f64; 3] {
        self.data.xipos().get(b.0).copied().unwrap_or([0.0; 3])
    }

    /// Centre of mass of the whole subtree rooted at a body, in the world frame.
    pub fn subtree_com(&self, b: BodyId) -> [f64; 3] {
        self.data
            .subtree_com()
            .get(b.0)
            .copied()
            .unwrap_or([0.0; 3])
    }

    /// Mass of a body in kilograms (not including children).
    pub fn body_mass(&self, b: BodyId) -> f64 {
        self.data
            .model()
            .body_mass()
            .get(b.0)
            .copied()
            .unwrap_or(0.0)
    }

    /// Total mass of the subtree rooted at a body, in kilograms.
    pub fn subtree_mass(&self, b: BodyId) -> f64 {
        self.data
            .model()
            .body_subtreemass()
            .get(b.0)
            .copied()
            .unwrap_or(0.0)
    }

    /// Principal moments of inertia of a body (kg m^2), in its inertial frame.
    pub fn body_inertia(&self, b: BodyId) -> [f64; 3] {
        self.data
            .model()
            .body_inertia()
            .get(b.0)
            .copied()
            .unwrap_or([0.0; 3])
    }

    /// Linear velocity of a body's centre of mass in the world frame (m/s).
    pub fn body_linear_velocity(&self, b: BodyId) -> [f64; 3] {
        self.body_velocity(b)[3..6].try_into().unwrap_or([0.0; 3])
    }

    /// Angular velocity of a body in the world frame (rad/s).
    pub fn body_angular_velocity(&self, b: BodyId) -> [f64; 3] {
        self.body_velocity(b)[0..3].try_into().unwrap_or([0.0; 3])
    }

    /// `[angular, linear]` velocity of a body at its centre of mass, world frame.
    fn body_velocity(&self, b: BodyId) -> [f64; 6] {
        self.data
            .try_object_velocity(MjtObj::mjOBJ_BODY, b.0, false)
            .unwrap_or([0.0; 6])
    }

    /// The values of one sensor, or an empty slice for a bad id.
    pub fn sensor(&self, s: SensorId) -> &[f64] {
        let m = self.data.model();
        let (Some(&adr), Some(&dim)) = (m.sensor_adr().get(s.0), m.sensor_dim().get(s.0)) else {
            return &[];
        };
        let (adr, dim) = (adr as usize, dim as usize);
        self.data.sensordata().get(adr..adr + dim).unwrap_or(&[])
    }

    /// World pose of a site. The rotation is returned as a quaternion.
    pub fn site_pose(&self, s: SiteId) -> Option<Pose> {
        let pos = *self.data.site_xpos().get(s.0)?;
        let mat = *self.data.site_xmat().get(s.0)?;
        Some(Pose {
            pos,
            quat: quat_from_mat(&mat),
        })
    }

    /// The gravity vector of the model in m/s^2.
    pub fn gravity(&self) -> [f64; 3] {
        self.data.model().opt().gravity
    }

    /// Moment of inertia (kg m^2) of one body about the line through the world
    /// point `point` with direction `axis`, at the current pose. Uses the
    /// body's full inertia tensor and the parallel-axis theorem. Returns 0 for
    /// a bad id or a zero `axis`.
    pub fn moment_about_axis(&self, b: BodyId, point: [f64; 3], axis: [f64; 3]) -> f64 {
        let norm = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        let (Some(&lam), Some(r)) = (
            self.data.model().body_inertia().get(b.0),
            self.data.ximat().get(b.0),
        ) else {
            return 0.0;
        };
        if norm.is_nan() || norm <= 1e-12 {
            return 0.0;
        }
        let u = [axis[0] / norm, axis[1] / norm, axis[2] / norm];
        // Principal axis i in the world frame is column i of the row-major matrix.
        let mut about_com = 0.0;
        for (i, l) in lam.iter().enumerate() {
            let e = [r[i], r[3 + i], r[6 + i]];
            let c = e[0] * u[0] + e[1] * u[1] + e[2] * u[2];
            about_com += l * c * c;
        }
        let com = self.body_com(b);
        let d = [com[0] - point[0], com[1] - point[1], com[2] - point[2]];
        let along = d[0] * u[0] + d[1] * u[1] + d[2] * u[2];
        let perp2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - along * along;
        about_com + self.body_mass(b) * perp2.max(0.0)
    }

    /// Total mechanical energy (potential plus kinetic) in joules.
    /// Runs a full forward pass first, so it is slower than a plain read.
    pub fn total_energy(&mut self) -> f64 {
        self.data.forward();
        self.data.energy_pos();
        self.data.energy_vel();
        let e = self.data.energy();
        e[0] + e[1]
    }

    // ----- rays -------------------------------------------------------------

    /// Cast a ray and return the nearest hit among the geom groups in `groups`.
    /// `dir` need not be unit length. A zero `dir` returns `None`.
    pub fn ray(&mut self, from: [f64; 3], dir: [f64; 3], groups: GroupMask) -> Option<RayHit> {
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len.is_nan() || len <= 1e-9 {
            return None;
        }
        let unit = [dir[0] / len, dir[1] / len, dir[2] / len];
        let mut normal = [0.0; 3];
        let (geom, dist) = self.data.ray(
            &from,
            &unit,
            Some(&groups.to_array()),
            true,
            None,
            Some(&mut normal),
        );
        let geom = geom?;
        if dist < 0.0 {
            return None;
        }
        Some(RayHit {
            geom: GeomId(geom),
            distance: dist,
            point: [
                from[0] + unit[0] * dist,
                from[1] + unit[1] * dist,
                from[2] + unit[2] * dist,
            ],
            normal,
        })
    }

    /// The RGBA colour of a geom as defined in the MJCF.
    pub fn geom_rgba(&self, g: GeomId) -> [f32; 4] {
        self.data
            .model()
            .geom_rgba()
            .get(g.0)
            .copied()
            .unwrap_or([0.0; 4])
    }

    /// The body a geom belongs to.
    pub fn geom_body(&self, g: GeomId) -> Option<BodyId> {
        self.data
            .model()
            .geom_bodyid()
            .get(g.0)
            .map(|&b| BodyId(b as usize))
    }

    // ----- contacts and forces ---------------------------------------------

    /// All current contacts with their forces.
    pub fn contacts(&self) -> Vec<ContactInfo> {
        let mut out = Vec::with_capacity(self.data.ncon() as usize);
        self.contacts_into(&mut out);
        out
    }

    /// Like [`Physics::contacts`], but reuses `out` (cleared first).
    pub fn contacts_into(&self, out: &mut Vec<ContactInfo>) {
        out.clear();
        for (i, c) in self.data.contact().iter().enumerate() {
            // Contacts excluded from the solver carry no force.
            let f = self.data.contact_force(i);
            out.push(ContactInfo {
                pos: c.pos,
                normal: [c.frame[0], c.frame[1], c.frame[2]],
                normal_force: f[0],
                friction_force: [f[1], f[2]],
                dist: c.dist,
                geom1: GeomId(c.geom1 as usize),
                geom2: GeomId(c.geom2 as usize),
            });
        }
    }

    /// Apply a force (newtons, world frame) to a body at a world point. It
    /// replaces any earlier force on that body and stays until replaced or
    /// cleared with [`Physics::clear_applied_forces`].
    pub fn apply_force(&mut self, b: BodyId, point: [f64; 3], force: [f64; 3]) {
        let com = self.body_com(b);
        let r = [point[0] - com[0], point[1] - com[1], point[2] - com[2]];
        let torque = [
            r[1] * force[2] - r[2] * force[1],
            r[2] * force[0] - r[0] * force[2],
            r[0] * force[1] - r[1] * force[0],
        ];
        if let Some(x) = self.data.xfrc_applied_mut().get_mut(b.0) {
            *x = [
                force[0], force[1], force[2], torque[0], torque[1], torque[2],
            ];
        }
    }

    /// Remove every applied force.
    pub fn clear_applied_forces(&mut self) {
        for x in self.data.xfrc_applied_mut().iter_mut() {
            *x = [0.0; 6];
        }
    }

    // ----- warnings ---------------------------------------------------------

    /// MuJoCo's warning counters since the last reset.
    pub fn warnings(&self) -> PhysicsWarnings {
        let w = self.data.warning();
        let n = |k: MjtWarning| w[k as usize].number.max(0) as u32;
        PhysicsWarnings {
            bad_qpos: n(MjtWarning::mjWARN_BADQPOS),
            bad_qvel: n(MjtWarning::mjWARN_BADQVEL),
            bad_qacc: n(MjtWarning::mjWARN_BADQACC),
            bad_ctrl: n(MjtWarning::mjWARN_BADCTRL),
            contact_full: n(MjtWarning::mjWARN_CONTACTFULL),
            constraint_full: n(MjtWarning::mjWARN_CNSTRFULL),
            inertia: n(MjtWarning::mjWARN_INERTIA),
        }
    }

    // ----- state ------------------------------------------------------------

    /// Copy out the full integration state (time, positions, velocities,
    /// warm start, controls, applied forces, ...).
    pub fn save_state(&self) -> PhysicsState {
        PhysicsState {
            values: self.data.state(STATE_SPEC).to_vec(),
        }
    }

    /// Restore a state saved from this same model, then recompute derived
    /// values. Stepping on from here reproduces the original run exactly.
    pub fn restore_state(&mut self, state: &PhysicsState) -> Result<(), PhysicsError> {
        let expected = self.data.model().state_size(STATE_SPEC);
        if state.values.len() != expected {
            return Err(PhysicsError::StateSize {
                got: state.values.len(),
                expected,
            });
        }
        self.data
            .set_state(&state.values, STATE_SPEC)
            .map_err(|e| PhysicsError::Restore(e.to_string()))?;
        self.data.forward();
        Ok(())
    }

    /// All generalized positions (`qpos`), for hashing a run. A free joint
    /// takes 7 values: position then quaternion.
    pub fn qpos(&self) -> &[f64] {
        self.data.qpos()
    }

    /// All generalized velocities (`qvel`). A free joint takes 6 values:
    /// linear then angular velocity.
    pub fn qvel(&self) -> &[f64] {
        self.data.qvel()
    }

    /// Set one generalized velocity. Out-of-range indices are ignored.
    pub fn set_qvel(&mut self, index: usize, value: f64) {
        if let Some(v) = self.data.qvel_mut().get_mut(index) {
            *v = value;
        }
    }
}

/// Rotation matrix (row-major 3x3) to unit quaternion `[w, x, y, z]`.
fn quat_from_mat(m: &[f64; 9]) -> [f64; 4] {
    let tr = m[0] + m[4] + m[8];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [
            0.25 * s,
            (m[7] - m[5]) / s,
            (m[2] - m[6]) / s,
            (m[3] - m[1]) / s,
        ]
    } else if m[0] > m[4] && m[0] > m[8] {
        let s = (1.0 + m[0] - m[4] - m[8]).sqrt() * 2.0;
        [
            (m[7] - m[5]) / s,
            0.25 * s,
            (m[1] + m[3]) / s,
            (m[2] + m[6]) / s,
        ]
    } else if m[4] > m[8] {
        let s = (1.0 + m[4] - m[0] - m[8]).sqrt() * 2.0;
        [
            (m[2] - m[6]) / s,
            (m[1] + m[3]) / s,
            0.25 * s,
            (m[5] + m[7]) / s,
        ]
    } else {
        let s = (1.0 + m[8] - m[0] - m[4]).sqrt() * 2.0;
        [
            (m[3] - m[1]) / s,
            (m[2] + m[6]) / s,
            (m[5] + m[7]) / s,
            0.25 * s,
        ]
    };
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// A tiny model with every kind of object the API touches.
    const SCENE: &str = r#"
<mujoco>
  <compiler angle="radian" inertiafromgeom="true" autolimits="true"/>
  <option timestep="0.001" integrator="implicitfast" cone="elliptic" impratio="10" gravity="0 0 -9.80665"/>
  <worldbody>
    <geom name="floor" type="plane" size="5 5 0.1" contype="2" conaffinity="1" group="0" rgba="0.1 0.2 0.3 1"/>
    <geom name="wall" type="box" pos="1 0 0.1" size="0.01 1 0.1" contype="2" conaffinity="1" group="0" rgba="1 0 0 1"/>
    <body name="ball" pos="0 0 0.5">
      <freejoint name="ball_free"/>
      <geom name="ball_geom" type="sphere" size="0.02" mass="0.1" group="1" contype="1" conaffinity="2"/>
      <site name="tip" pos="0 0 0.02" euler="0 1.5707963267948966 0"/>
    </body>
    <body name="arm" pos="0 -0.5 1.0">
      <joint name="hinge" type="hinge" axis="0 1 0" damping="0.001"/>
      <geom name="rod" type="capsule" fromto="0 0 0 0.2 0 0" size="0.01" mass="0.05" group="1" contype="0" conaffinity="0"/>
    </body>
  </worldbody>
  <actuator>
    <general name="motor" joint="hinge" gainprm="0.1" biasprm="0 0 -0.01" ctrlrange="-8 8"/>
  </actuator>
  <sensor>
    <jointpos name="hinge_pos" joint="hinge"/>
    <jointvel name="hinge_vel" joint="hinge"/>
  </sensor>
</mujoco>"#;

    fn scene() -> Physics {
        Physics::from_mjcf(SCENE).unwrap()
    }

    #[test]
    fn from_mjcf_loads_and_rejects_bad_xml() {
        assert!(Physics::from_mjcf(SCENE).is_ok());
        let err = Physics::from_mjcf("<mujoco><oops/></mujoco>").unwrap_err();
        assert!(matches!(err, PhysicsError::Load(_)), "{err}");
        assert!(Physics::from_mjcf("not xml at all").is_err());
    }

    #[test]
    fn step_advances_time_by_one_timestep() {
        let mut p = scene();
        assert_eq!(p.time(), 0.0);
        assert_relative_eq!(p.timestep(), 0.001);
        p.step();
        assert_relative_eq!(p.time(), 0.001, epsilon = 1e-12);
        for _ in 0..99 {
            p.step();
        }
        assert_relative_eq!(p.time(), 0.1, epsilon = 1e-9);
    }

    #[test]
    fn reset_returns_to_the_start() {
        let mut p = scene();
        let start = p.body_pose(p.body_id("ball").unwrap());
        for _ in 0..200 {
            p.step();
        }
        p.reset();
        assert_eq!(p.time(), 0.0);
        assert_eq!(p.body_pose(p.body_id("ball").unwrap()), start);
    }

    #[test]
    fn forward_refreshes_poses_after_a_manual_change() {
        let mut p = scene();
        let j = p.joint_id("hinge").unwrap();
        let tip = p.body_id("arm").unwrap();
        let before = p.body_pose(tip).quat;
        p.set_joint_pos(j, 0.5);
        p.forward();
        assert_ne!(p.body_pose(tip).quat, before);
    }

    #[test]
    fn counts_match_the_model() {
        let p = scene();
        assert_eq!(p.body_count(), 3); // world, ball, arm
        assert_eq!(p.geom_count(), 4);
        assert_eq!(p.joint_count(), 2);
        assert_eq!(p.actuator_count(), 1);
        assert_eq!(p.sensor_count(), 2);
    }

    #[test]
    fn name_lookups_work_and_fail_clearly() {
        let p = scene();
        assert_eq!(p.body_id("arm").unwrap(), BodyId(2));
        assert_eq!(p.joint_id("hinge").unwrap(), JointId(1));
        assert_eq!(p.actuator_id("motor").unwrap(), ActuatorId(0));
        assert_eq!(p.sensor_id("hinge_vel").unwrap(), SensorId(1));
        assert_eq!(p.geom_id("wall").unwrap(), GeomId(1));
        assert_eq!(p.site_id("tip").unwrap(), SiteId(0));
        let err = p.body_id("nope").unwrap_err();
        assert_eq!(err.to_string(), "the model has no body called \"nope\"");
        assert!(p.joint_id("nope").is_err());
        assert!(p.actuator_id("nope").is_err());
        assert!(p.sensor_id("nope").is_err());
        assert!(p.geom_id("nope").is_err());
        assert!(p.site_id("nope").is_err());
    }

    #[test]
    fn ctrl_drives_a_joint_and_readings_follow() {
        let mut p = scene();
        let a = p.actuator_id("motor").unwrap();
        let j = p.joint_id("hinge").unwrap();
        p.set_ctrl(a, 5.0);
        for _ in 0..200 {
            p.step();
        }
        assert!(p.joint_vel(j) > 0.1, "vel {}", p.joint_vel(j));
        assert!(p.joint_pos(j).abs() > 1e-3);
        // Bad ids are ignored, not panics.
        p.set_ctrl(ActuatorId(99), 1.0);
        assert_eq!(p.joint_pos(JointId(99)), 0.0);
        assert_eq!(p.joint_vel(JointId(99)), 0.0);
    }

    #[test]
    fn set_joint_pos_moves_the_joint() {
        let mut p = scene();
        let j = p.joint_id("hinge").unwrap();
        p.set_joint_pos(j, 0.25);
        assert_relative_eq!(p.joint_pos(j), 0.25);
        p.set_joint_pos(JointId(99), 1.0); // ignored
    }

    #[test]
    fn body_pose_com_and_masses() {
        let p = scene();
        let world = p.body_pose(BodyId(0));
        assert_eq!(world.pos, [0.0; 3]);
        assert_eq!(world.quat, [1.0, 0.0, 0.0, 0.0]);
        let ball = p.body_id("ball").unwrap();
        assert_relative_eq!(p.body_pose(ball).pos[2], 0.5);
        assert_relative_eq!(p.body_com(ball)[2], 0.5);
        assert_relative_eq!(p.body_mass(ball), 0.1);
        assert_relative_eq!(p.subtree_mass(ball), 0.1);
        let arm = p.body_id("arm").unwrap();
        // Capsule from x=0 to 0.2: centre of mass at x = 0.1.
        assert_relative_eq!(p.body_com(arm)[0], 0.1, epsilon = 1e-9);
        assert_relative_eq!(p.subtree_com(arm)[0], 0.1, epsilon = 1e-9);
        // World subtree holds every moving mass.
        assert_relative_eq!(p.subtree_mass(BodyId(0)), 0.15, epsilon = 1e-9);
        // Solid sphere: I = 2/5 m r^2.
        let i = p.body_inertia(ball);
        assert_relative_eq!(i[0], 0.4 * 0.1 * 0.02 * 0.02, epsilon = 1e-9);
        assert_eq!(p.body_mass(BodyId(99)), 0.0);
        assert_eq!(p.subtree_mass(BodyId(99)), 0.0);
        assert_eq!(p.body_inertia(BodyId(99)), [0.0; 3]);
        assert_eq!(p.body_com(BodyId(99)), [0.0; 3]);
        assert_eq!(p.subtree_com(BodyId(99)), [0.0; 3]);
        assert_eq!(p.body_pose(BodyId(99)).quat, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn moment_about_axis_uses_parallel_axis_theorem() {
        let p = scene();
        let ball = p.body_id("ball").unwrap();
        let com = p.body_com(ball);
        let about_com = p.moment_about_axis(ball, com, [0.0, 1.0, 0.0]);
        assert_relative_eq!(about_com, 0.4 * 0.1 * 0.02 * 0.02, epsilon = 1e-9);
        // 0.3 m from the centre: add m d^2.
        let off = [com[0] + 0.3, com[1], com[2]];
        let shifted = p.moment_about_axis(ball, off, [0.0, 1.0, 0.0]);
        assert_relative_eq!(shifted, about_com + 0.1 * 0.09, epsilon = 1e-9);
        // A point along the axis adds nothing.
        let along = [com[0], com[1] + 5.0, com[2]];
        assert_relative_eq!(
            p.moment_about_axis(ball, along, [0.0, 1.0, 0.0]),
            about_com,
            epsilon = 1e-12
        );
        assert_eq!(p.moment_about_axis(ball, com, [0.0; 3]), 0.0);
        assert_eq!(p.moment_about_axis(BodyId(99), com, [0.0, 0.0, 1.0]), 0.0);
    }

    #[test]
    fn gravity_is_the_header_value() {
        assert_eq!(scene().gravity(), [0.0, 0.0, -9.80665]);
    }

    #[test]
    fn velocities_of_a_falling_ball() {
        let mut p = scene();
        let ball = p.body_id("ball").unwrap();
        for _ in 0..100 {
            p.step();
        }
        p.forward();
        let v = p.body_linear_velocity(ball);
        assert_relative_eq!(v[2], -9.80665 * 0.1, epsilon = 2e-3);
        assert_relative_eq!(v[0], 0.0, epsilon = 1e-9);
        assert_eq!(p.body_angular_velocity(ball), [0.0; 3]);
        assert_eq!(p.body_linear_velocity(BodyId(99)), [0.0; 3]);
        assert_eq!(p.body_angular_velocity(BodyId(99)), [0.0; 3]);
    }

    #[test]
    fn sensors_read_joint_state() {
        let mut p = scene();
        let j = p.joint_id("hinge").unwrap();
        p.set_ctrl(p.actuator_id("motor").unwrap(), 3.0);
        for _ in 0..50 {
            p.step();
        }
        p.forward();
        let pos = p.sensor(p.sensor_id("hinge_pos").unwrap());
        assert_eq!(pos.len(), 1);
        assert_relative_eq!(pos[0], p.joint_pos(j), epsilon = 1e-9);
        assert_relative_eq!(p.sensor(SensorId(1))[0], p.joint_vel(j), epsilon = 1e-9);
        assert!(p.sensor(SensorId(99)).is_empty());
    }

    #[test]
    fn site_pose_follows_its_body() {
        let p = scene();
        let pose = p.site_pose(p.site_id("tip").unwrap()).unwrap();
        assert_relative_eq!(pose.pos[2], 0.52, epsilon = 1e-9);
        // 90 degrees about y: w = x... = cos(pi/4).
        assert_relative_eq!(
            pose.quat[0],
            std::f64::consts::FRAC_1_SQRT_2,
            epsilon = 1e-9
        );
        assert_relative_eq!(
            pose.quat[2],
            std::f64::consts::FRAC_1_SQRT_2,
            epsilon = 1e-9
        );
        assert!(p.site_pose(SiteId(9)).is_none());
    }

    #[test]
    fn quaternion_from_matrix_covers_all_branches() {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        // identity
        assert_eq!(
            quat_from_mat(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0, 0.0]
        );
        // 180 degrees about x, y and z hit the three other branches.
        let qx = quat_from_mat(&[1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, -1.0]);
        assert_relative_eq!(qx[1].abs(), 1.0, epsilon = 1e-12);
        let qy = quat_from_mat(&[-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0]);
        assert_relative_eq!(qy[2].abs(), 1.0, epsilon = 1e-12);
        let qz = quat_from_mat(&[-1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0]);
        assert_relative_eq!(qz[3].abs(), 1.0, epsilon = 1e-12);
        // 90 degrees about z
        let q = quat_from_mat(&[0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_relative_eq!(q[0], s, epsilon = 1e-12);
        assert_relative_eq!(q[3], s, epsilon = 1e-12);
    }

    #[test]
    fn total_energy_is_conserved_for_a_free_ball() {
        let mut p = Physics::from_mjcf(
            r#"<mujoco><option timestep="0.001" gravity="0 0 -9.80665"/><worldbody>
            <body pos="0 0 10"><freejoint/><geom type="sphere" size="0.05" mass="2"/></body>
            </worldbody></mujoco>"#,
        )
        .unwrap();
        let e0 = p.total_energy();
        for _ in 0..300 {
            p.step();
        }
        // The ball falls, trading potential for kinetic energy.
        assert_relative_eq!(p.total_energy(), e0, max_relative = 1e-3, epsilon = 0.05);
    }

    #[test]
    fn ray_hits_the_floor_with_a_normal() {
        let mut p = scene();
        let hit = p
            .ray(
                [0.5, 0.5, 1.0],
                [0.0, 0.0, -2.0],
                GroupMask::WORLD_AND_OBJECTS,
            )
            .unwrap();
        assert_eq!(hit.geom, p.geom_id("floor").unwrap());
        assert_relative_eq!(hit.distance, 1.0, epsilon = 1e-9);
        assert_relative_eq!(hit.point[2], 0.0, epsilon = 1e-9);
        assert_relative_eq!(hit.normal[2], 1.0, epsilon = 1e-9);
        // Sideways at the wall: the normal faces back towards the ray.
        let wall = p
            .ray(
                [0.0, 0.0, 0.1],
                [1.0, 0.0, 0.0],
                GroupMask::WORLD_AND_OBJECTS,
            )
            .unwrap();
        assert_eq!(wall.geom, p.geom_id("wall").unwrap());
        assert_relative_eq!(wall.distance, 0.99, epsilon = 1e-9);
        assert_relative_eq!(wall.normal[0], -1.0, epsilon = 1e-9);
    }

    #[test]
    fn ray_respects_geom_groups() {
        let mut p = scene();
        // The ball is in group 1: invisible to robot sensors, visible with ALL.
        let from = [0.0, 0.0, 0.7];
        let down = [0.0, 0.0, -1.0];
        let seen = p.ray(from, down, GroupMask::ALL).unwrap();
        assert_eq!(seen.geom, p.geom_id("ball_geom").unwrap());
        let sensor = p.ray(from, down, GroupMask::WORLD_AND_OBJECTS).unwrap();
        assert_eq!(sensor.geom, p.geom_id("floor").unwrap());
        assert_eq!(GroupMask::WORLD_AND_OBJECTS.to_array(), [1, 0, 1, 0, 0, 0]);
        assert_eq!(GroupMask::ALL.to_array(), [1; 6]);
    }

    #[test]
    fn ray_misses_and_degenerate_directions_return_none() {
        let mut p = scene();
        assert!(
            p.ray([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], GroupMask::ALL)
                .is_none()
        );
        assert!(p.ray([0.0, 0.0, 1.0], [0.0; 3], GroupMask::ALL).is_none());
        assert!(
            p.ray([0.0, 0.0, 1.0], [f64::NAN; 3], GroupMask::ALL)
                .is_none()
        );
    }

    #[test]
    fn geom_properties() {
        let p = scene();
        let floor = p.geom_id("floor").unwrap();
        assert_eq!(p.geom_rgba(floor), [0.1, 0.2, 0.3, 1.0]);
        assert_eq!(p.geom_rgba(GeomId(99)), [0.0; 4]);
        assert_eq!(p.geom_body(floor), Some(BodyId(0)));
        assert_eq!(p.geom_body(p.geom_id("rod").unwrap()), Some(BodyId(2)));
        assert_eq!(p.geom_body(GeomId(99)), None);
    }

    /// A box resting on the floor.
    fn resting_box() -> Physics {
        Physics::from_mjcf(
            r#"<mujoco><option timestep="0.001" integrator="implicitfast" cone="elliptic" impratio="10"/>
            <default><geom solref="0.004 1" solimp="0.95 0.99 0.001"/></default>
            <worldbody><geom name="floor" type="plane" size="5 5 0.1" contype="2" conaffinity="1"/>
            <body name="box" pos="0 0 0.05"><freejoint/>
            <geom name="box" type="box" size="0.05 0.05 0.05" mass="2" contype="1" conaffinity="2"/></body>
            </worldbody></mujoco>"#,
        )
        .unwrap()
    }

    #[test]
    fn contacts_carry_the_supporting_force() {
        let mut p = resting_box();
        for _ in 0..1000 {
            p.step();
        }
        let contacts = p.contacts();
        assert!(!contacts.is_empty());
        let total: f64 = contacts.iter().map(|c| c.normal_force).sum();
        assert_relative_eq!(total, 2.0 * 9.80665, max_relative = 0.01);
        for c in &contacts {
            assert!(c.normal[2].abs() > 0.99);
            assert!(c.pos[2].abs() < 1e-3);
            assert!(c.dist <= 1e-6);
            assert_ne!(c.geom1, c.geom2);
        }
        let mut buf = vec![contacts[0]; 7];
        p.contacts_into(&mut buf);
        assert_eq!(buf.len(), contacts.len());
    }

    #[test]
    fn applied_force_pushes_and_clears() {
        let mut p = Physics::from_mjcf(
            r#"<mujoco><option timestep="0.001" gravity="0 0 0"/><worldbody>
            <body name="b" pos="0 0 1"><freejoint/><geom type="box" size="0.05 0.05 0.05" mass="1"/></body>
            </worldbody></mujoco>"#,
        )
        .unwrap();
        let b = p.body_id("b").unwrap();
        // 1 N for 1 s on 1 kg: v = 1 m/s.
        p.apply_force(b, p.body_com(b), [1.0, 0.0, 0.0]);
        for _ in 0..1000 {
            p.step();
        }
        p.forward();
        assert_relative_eq!(p.body_linear_velocity(b)[0], 1.0, epsilon = 2e-3);
        assert_relative_eq!(p.body_angular_velocity(b)[2], 0.0, epsilon = 1e-9);
        p.clear_applied_forces();
        for _ in 0..100 {
            p.step();
        }
        p.forward();
        assert_relative_eq!(p.body_linear_velocity(b)[0], 1.0, epsilon = 2e-3);
        // Off-centre force makes torque: r = +x, F = +y gives +z torque.
        p.reset();
        let com = p.body_com(b);
        p.apply_force(b, [com[0] + 0.1, com[1], com[2]], [0.0, 1.0, 0.0]);
        for _ in 0..100 {
            p.step();
        }
        p.forward();
        assert!(p.body_angular_velocity(b)[2] > 0.0);
        p.apply_force(BodyId(99), [0.0; 3], [1.0; 3]); // ignored
    }

    #[test]
    fn warnings_start_clean_and_flag_a_nan_control() {
        let mut p = scene();
        assert_eq!(p.warnings(), PhysicsWarnings::default());
        assert!(!p.warnings().is_unstable());
        p.set_ctrl(p.actuator_id("motor").unwrap(), f64::NAN);
        p.step();
        let w = p.warnings();
        assert!(w.bad_ctrl > 0, "{w:?}");
        assert!(w.is_unstable());
    }

    #[test]
    fn restored_state_continues_bit_for_bit() {
        let mut p = scene();
        p.set_ctrl(p.actuator_id("motor").unwrap(), 2.0);
        for _ in 0..300 {
            p.step();
        }
        let saved = p.save_state();
        let mut first = Vec::new();
        for _ in 0..500 {
            p.step();
            first.push(p.qpos().to_vec());
        }
        p.restore_state(&saved).unwrap();
        assert_relative_eq!(p.time(), 0.3, epsilon = 1e-9);
        let mut second = Vec::new();
        for _ in 0..500 {
            p.step();
            second.push(p.qpos().to_vec());
        }
        assert_eq!(first, second);
        assert!(!saved.as_slice().is_empty());
    }

    #[test]
    fn restoring_a_foreign_state_is_an_error() {
        let mut p = scene();
        let other = resting_box().save_state();
        let err = p.restore_state(&other).unwrap_err();
        assert!(matches!(err, PhysicsError::StateSize { .. }), "{err}");
    }

    #[test]
    fn qvel_can_be_read_and_set() {
        let mut p = scene();
        assert_eq!(p.qvel().len(), 7);
        p.set_qvel(0, 2.0);
        p.set_qvel(99, 5.0); // ignored
        assert_eq!(p.qvel()[0], 2.0);
        for _ in 0..10 {
            p.step();
        }
        assert!((p.qpos()[0] - 0.02).abs() < 1e-3);
    }

    #[test]
    fn debug_output_names_the_type() {
        assert!(format!("{:?}", scene()).starts_with("Physics"));
    }
}
