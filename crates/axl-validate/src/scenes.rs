//! MJCF for the simple test scenes used by the physics checks.
//!
//! They use the fixed header from `SPEC.md` ("Fixed MJCF header") so the
//! tests exercise the same solver settings the real robots will.

/// The spec's fixed MJCF header, opening `<mujoco>` and the defaults.
/// `solref`/`solimp` are the calibration starting values from the spec.
pub const HEADER: &str = r#"<mujoco model="axl-test">
  <compiler angle="radian" inertiafromgeom="true" autolimits="true"/>
  <option timestep="0.001" integrator="implicitfast" cone="elliptic" impratio="10" gravity="0 0 -9.80665"/>
  <default>
    <geom condim="3" solref="0.004 1" solimp="0.95 0.99 0.001"/>
  </default>
"#;

/// Wrap a `<worldbody>` (and anything after it) in the header.
fn model(body: &str) -> String {
    format!("{HEADER}{body}\n</mujoco>\n")
}

/// A small sphere above a plane. Centre starts `drop_height + radius` up.
pub fn free_fall(drop_height: f64, radius: f64) -> String {
    model(&format!(
        r#"<worldbody>
    <geom name="floor" type="plane" size="5 5 0.1"/>
    <body name="ball" pos="0 0 {z}"><freejoint name="ball_free"/>
      <geom name="ball" type="sphere" size="{radius}" mass="0.01"/>
    </body>
  </worldbody>"#,
        z = drop_height + radius
    ))
}

/// A physical pendulum: a rod with a bob, hinged about the world y axis at
/// height 1 m. Its geoms do not collide with anything.
pub fn pendulum() -> String {
    model(
        r#"<worldbody>
    <body name="pend" pos="0 0 1">
      <joint name="hinge" type="hinge" axis="0 1 0" damping="0" frictionloss="0" armature="0"/>
      <geom name="rod" type="box" pos="0 0 -0.25" size="0.01 0.01 0.25" mass="0.3" contype="0" conaffinity="0"/>
      <geom name="bob" type="sphere" pos="0 0 -0.5" size="0.04" mass="0.7" contype="0" conaffinity="0"/>
    </body>
  </worldbody>"#,
    )
}

/// A static slope of `angle` radians (rotation about y) with a free body on
/// it. `shape` is the geom XML for the body, with its axes aligned to the
/// slope: +x points down the slope, +z is the slope normal. `centre_height`
/// is how far the body's centre sits above the slope surface.
pub fn slope_with(angle: f64, friction: f64, shape: &str, centre_height: f64) -> String {
    let (s, c) = angle.sin_cos();
    model(&format!(
        r#"<worldbody>
    <geom name="slope" type="plane" size="5 5 0.1" euler="0 {angle} 0" friction="{friction} 0.005 0.0001"/>
    <body name="obj" pos="{px} 0 {pz}" euler="0 {angle} 0"><freejoint name="obj_free"/>
      {shape}
    </body>
  </worldbody>"#,
        px = s * centre_height,
        pz = c * centre_height
    ))
}

/// A cube on a tilted plane (see [`slope_with`]).
pub fn box_on_slope(angle: f64, mu: f64) -> String {
    let half = 0.025;
    slope_with(
        angle,
        mu,
        &format!(
            r#"<geom name="obj" type="box" size="{half} {half} {half}" mass="0.1" friction="{mu} 0.005 0.0001"/>"#
        ),
        half,
    )
}

/// A flat floor with a cube on it. The cube's friction is `mu`.
pub fn box_on_floor(mu: f64) -> String {
    box_on_slope(0.0, mu)
}

/// A solid cylinder (axis along y) on a slope; friction high enough to roll
/// without slipping.
pub fn cylinder_on_slope(angle: f64, radius: f64) -> String {
    slope_with(
        angle,
        1.0,
        &format!(
            r#"<geom name="obj" type="cylinder" size="{radius} 0.02" zaxis="0 1 0" mass="0.2" friction="1 0.005 0.0001"/>"#
        ),
        radius,
    )
}

/// A solid sphere on a slope; friction high enough to roll without slipping.
pub fn sphere_on_slope(angle: f64, radius: f64) -> String {
    slope_with(
        angle,
        1.0,
        &format!(
            r#"<geom name="obj" type="sphere" size="{radius}" mass="0.2" friction="1 0.005 0.0001"/>"#
        ),
        radius,
    )
}

/// A busier scene for the determinism test: a floor, `n` boxes and spheres
/// with starting positions from `values` (each in 0..1), and a hinged pendulum.
pub fn clutter(n: usize, mut values: impl FnMut() -> f64) -> String {
    let mut bodies = String::new();
    for i in 0..n {
        let x = (values() - 0.5) * 0.6;
        let y = (values() - 0.5) * 0.6;
        let z = 0.1 + 0.15 * i as f64;
        let geom = if i % 2 == 0 {
            r#"<geom type="box" size="0.04 0.03 0.02" mass="0.1"/>"#
        } else {
            r#"<geom type="sphere" size="0.03" mass="0.08"/>"#
        };
        bodies.push_str(&format!(
            "<body name=\"b{i}\" pos=\"{x} {y} {z}\"><freejoint/>{geom}</body>\n    "
        ));
    }
    model(&format!(
        r#"<worldbody>
    <geom name="floor" type="plane" size="5 5 0.1"/>
    {bodies}
    <body name="pend" pos="1 0 1"><joint name="hinge" type="hinge" axis="0 1 0" damping="0.0005"/>
      <geom type="capsule" fromto="0 0 0 0 0 -0.4" size="0.01" mass="0.2" contype="0" conaffinity="0"/></body>
  </worldbody>"#
    ))
}
