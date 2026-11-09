//! Small MJCF rigs with hand-made device maps, for this crate's tests and the accuracy suite
//! (`axl-validate`). They use the spec's fixed MJCF header and the same actuator expression the
//! compiler writes, so the hardware models see the same kind of model as in a compiled robot.

use std::f64::consts::FRAC_PI_2;
use std::fmt::Write;

use axl_build::Port;
use axl_compile::params::MotorParams;
use axl_compile::{DeviceEntry, DeviceMap, NamedId, SensorEntry, SensorRole};
use axl_mujoco::Physics;
use axl_parts::types::DeviceModel;

const HEADER: &str = r#"<mujoco model="axl-hw-rig">
  <compiler angle="radian" inertiafromgeom="true" autolimits="true"/>
  <option timestep="0.001" integrator="implicitfast" cone="elliptic" impratio="10" gravity="0 0 -9.80665"/>
  <default>
    <geom condim="3" solref="0.004 1" solimp="0.95 0.99 0.001"/>
  </default>
"#;

/// A physics model together with the device map describing it.
pub struct Rig {
    /// The loaded model.
    pub physics: Physics,
    /// Devices by name.
    pub map: DeviceMap,
}

/// One motor of a [`motor_rig`].
#[derive(Clone, Copy, Debug)]
pub struct MotorSpec {
    /// Device name.
    pub name: &'static str,
    /// Medium, large or servo.
    pub model: DeviceModel,
    /// Its parameters.
    pub params: MotorParams,
    /// Hold the shaft still with an equality constraint (blocked-rotor tests).
    pub locked: bool,
}

/// Mass and radius of the rotor disc, which sets its own inertia (0.5 m r^2).
pub const ROTOR_MASS: f64 = 0.05;
/// Radius of the rotor disc, m.
pub const ROTOR_RADIUS: f64 = 0.02;

fn named(name: &str, id: usize) -> NamedId {
    NamedId {
        name: name.to_owned(),
        id,
    }
}

/// One free-turning (or locked) motor per spec, each a rotor disc on a vertical hinge fixed in
/// the world, with the compiler's `<general>` DC-motor actuator and the joint damping, friction
/// loss and armature of its parameters.
pub fn motor_rig(specs: &[MotorSpec], damping: f64) -> Result<Rig, String> {
    let mut xml = String::from(HEADER);
    xml.push_str("  <worldbody>\n");
    for (i, s) in specs.iter().enumerate() {
        let range = if s.model == DeviceModel::Servo {
            format!(" range=\"{} {}\"", -FRAC_PI_2, FRAC_PI_2)
        } else {
            String::new()
        };
        let _ = writeln!(
            xml,
            "    <body name=\"rotor_{n}\" pos=\"0 {y} 1\"><joint name=\"j_{n}\" type=\"hinge\" axis=\"0 0 1\" damping=\"{damping}\" frictionloss=\"{fl}\" armature=\"{arm}\"{range}/><geom type=\"cylinder\" size=\"{r} 0.01\" mass=\"{m}\" contype=\"0\" conaffinity=\"0\" group=\"1\"/></body>",
            n = s.name,
            y = 0.2 * i as f64,
            fl = s.params.friction_loss,
            arm = s.params.armature,
            r = ROTOR_RADIUS,
            m = ROTOR_MASS,
        );
    }
    xml.push_str("  </worldbody>\n");
    if specs.iter().any(|s| s.locked) {
        xml.push_str("  <equality>\n");
        for s in specs.iter().filter(|s| s.locked) {
            let _ = writeln!(
                xml,
                "    <joint name=\"lock_{n}\" joint1=\"j_{n}\" polycoef=\"0 0 0 0 0\"/>",
                n = s.name
            );
        }
        xml.push_str("  </equality>\n");
    }
    xml.push_str("  <actuator>\n");
    for s in specs {
        let m = &s.params;
        let force = m.stall_torque * m.v_max / m.v_nom;
        let _ = writeln!(
            xml,
            "    <general name=\"a_{n}\" joint=\"j_{n}\" gaintype=\"fixed\" biastype=\"affine\" gainprm=\"{g}\" biasprm=\"0 0 {b}\" ctrlrange=\"{lo} {hi}\" forcerange=\"{flo} {fhi}\"/>",
            n = s.name,
            g = m.stall_torque / m.v_nom,
            b = -m.stall_torque / m.omega0,
            lo = -m.v_max,
            hi = m.v_max,
            flo = -force,
            fhi = force,
        );
    }
    xml.push_str("  </actuator>\n</mujoco>\n");
    let physics = Physics::from_mjcf(&xml).map_err(|e| e.to_string())?;
    let mut map = DeviceMap::default();
    for (i, s) in specs.iter().enumerate() {
        let j = physics
            .joint_id(&format!("j_{}", s.name))
            .map_err(|e| e.to_string())?;
        let a = physics
            .actuator_id(&format!("a_{}", s.name))
            .map_err(|e| e.to_string())?;
        map.devices.push(DeviceEntry {
            name: s.name.to_owned(),
            uid: i as u32 + 1,
            port: Port::ALL.get(i).copied(),
            kind: s.model,
            joint: Some(named(&format!("j_{}", s.name), j.0)),
            actuator: Some(named(&format!("a_{}", s.name), a.0)),
            site: None,
            sensors: Vec::new(),
            motor: Some(s.params),
        });
    }
    Ok(Rig { physics, map })
}

/// What the world around the sensors looks like.
#[derive(Clone, Copy, Debug)]
pub struct WorldSpec {
    /// Distance from the sensor body's origin to the wall face along +x, m.
    pub wall_distance: f64,
    /// Rotation of the wall about the vertical, radians (0 = square to the sensors).
    pub wall_angle: f64,
    /// Mass resting on the touch plate, kg (0 for none).
    pub weight_kg: f64,
    /// Floor colour (rgb 0..1).
    pub floor_rgb: [f64; 3],
    /// Gravity on (false for a free-spinning gyro test).
    pub gravity: bool,
}

impl Default for WorldSpec {
    fn default() -> Self {
        Self {
            wall_distance: 0.5,
            wall_angle: 0.0,
            weight_kg: 0.0,
            floor_rgb: [0.5, 0.5, 0.5],
            gravity: true,
        }
    }
}

/// A hub on a vertical hinge `spin` (with site `imu`, a gyroscope and an accelerometer and a
/// forward-looking site `eye` for the distance sensors), a colour probe on a slide joint `slide`
/// 8 mm above the floor looking down (site `look`), and a touch plate with a mass on it. The device
/// map holds the hub plus `eye` (ultrasonic), `tof`, `color` and `touch` sensors.
pub fn world_rig(w: &WorldSpec) -> Result<Rig, String> {
    let mut xml = String::from(HEADER);
    if !w.gravity {
        xml = xml.replace("gravity=\"0 0 -9.80665\"", "gravity=\"0 0 0\"");
    }
    let (s, c) = w.wall_angle.sin_cos();
    // The wall face nearest the sensors passes through (distance, 0); the box is 2 cm thick.
    let centre = [w.wall_distance + 0.01 * c, 0.01 * s];
    let _ = write!(
        xml,
        r#"  <worldbody>
    <geom name="floor" type="plane" size="5 5 0.1" contype="2" conaffinity="1" group="0" rgba="{} {} {} 1"/>
    <geom name="wall" type="box" pos="{} {} 0.5" euler="0 0 {}" size="0.01 3 0.5" contype="2" conaffinity="1" group="0" rgba="0.9 0.2 0.2 1"/>
    <body name="hub" pos="0 0 0.1">
      <joint name="spin" type="hinge" axis="0 0 1" damping="0"/>
      <geom type="box" size="0.02 0.02 0.01" mass="0.1" contype="0" conaffinity="0" group="1"/>
      <site name="imu" size="0.002"/>
      <site name="eye" size="0.002" zaxis="1 0 0"/>
    </body>
    <body name="probe" pos="0 -0.4 0.008">
      <joint name="slide" type="slide" axis="1 0 0" damping="0"/>
      <geom type="box" size="0.005 0.005 0.002" mass="0.01" contype="0" conaffinity="0" group="1"/>
      <site name="look" size="0.002" zaxis="0 0 -1"/>
    </body>
    <geom name="plate" type="box" pos="0 1 0.02" size="0.05 0.05 0.02" contype="2" conaffinity="1" group="0"/>
    <site name="tap" type="box" pos="0 1 0.04" size="0.06 0.06 0.004"/>
"#,
        w.floor_rgb[0], w.floor_rgb[1], w.floor_rgb[2], centre[0], centre[1], w.wall_angle
    );
    if w.weight_kg > 0.0 {
        let _ = writeln!(
            xml,
            "    <body name=\"weight\" pos=\"0 1 0.0605\"><freejoint/><geom type=\"box\" size=\"0.03 0.03 0.02\" mass=\"{}\" contype=\"3\" conaffinity=\"3\" group=\"2\"/></body>",
            w.weight_kg
        );
    }
    xml.push_str(
        r#"  </worldbody>
  <sensor>
    <gyro name="gyro" site="imu"/>
    <accelerometer name="accel" site="imu"/>
    <touch name="touch_sensor" site="tap"/>
  </sensor>
</mujoco>
"#,
    );
    let physics = Physics::from_mjcf(&xml).map_err(|e| e.to_string())?;
    let site = |n: &str| -> Result<NamedId, String> {
        Ok(named(n, physics.site_id(n).map_err(|e| e.to_string())?.0))
    };
    let sensor = |n: &str, role| -> Result<SensorEntry, String> {
        Ok(SensorEntry {
            role,
            sensor: named(n, physics.sensor_id(n).map_err(|e| e.to_string())?.0),
        })
    };
    let dev = |name: &str, port, kind, site, sensors| DeviceEntry {
        name: name.to_owned(),
        uid: 0,
        port,
        kind,
        joint: None,
        actuator: None,
        site,
        sensors,
        motor: None,
    };
    let map = DeviceMap {
        devices: vec![
            dev(
                "hub",
                None,
                DeviceModel::Hub,
                Some(site("imu")?),
                vec![
                    sensor("gyro", SensorRole::Gyro)?,
                    sensor("accel", SensorRole::Accel)?,
                ],
            ),
            dev(
                "eye",
                Some(Port::A),
                DeviceModel::Ultrasonic,
                Some(site("eye")?),
                vec![],
            ),
            dev(
                "tof",
                Some(Port::B),
                DeviceModel::TimeOfFlight,
                Some(site("eye")?),
                vec![],
            ),
            dev(
                "color",
                Some(Port::C),
                DeviceModel::Color,
                Some(site("look")?),
                vec![],
            ),
            dev(
                "touch",
                Some(Port::D),
                DeviceModel::Touch,
                Some(site("tap")?),
                vec![sensor("touch_sensor", SensorRole::Touch)?],
            ),
        ],
    };
    Ok(Rig { physics, map })
}
