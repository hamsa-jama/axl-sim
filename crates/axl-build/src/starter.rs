//! The starter robots and the m12 reference design, built through the same commands the UI issues.
//!
//! Each step names a part, how the user turns it, and where the cursor points. Positions are never
//! written down: the snapping code finds them, exactly as it does when a learner builds the robot.
//! `assets/robots/*.axlbot` are generated from these recipes and a test checks they still match.

use axl_parts::catalog::Catalog;

use crate::builder::{BuildError, Builder};
use crate::model::{Assembly, Port};
use crate::orient::TurnAxis;
use crate::place::Ray;

/// Where the cursor ray comes from, aimed at a point (mm, robot frame).
#[derive(Clone, Copy, Debug)]
pub enum Aim {
    /// From above, straight down at (x, y).
    Down(f64, f64),
    /// From the +Y side, heading -Y, at (x, z).
    FromPosY(f64, f64),
    /// From the -Y side, heading +Y, at (x, z).
    FromNegY(f64, f64),
}

impl Aim {
    fn ray(self) -> Ray {
        let (origin, dir) = match self {
            Aim::Down(x, y) => ([x, y, 400.0], [0.0, 0.0, -1.0]),
            Aim::FromPosY(x, z) => ([x, 400.0, z], [0.0, -1.0, 0.0]),
            Aim::FromNegY(x, z) => ([x, -400.0, z], [0.0, 1.0, 0.0]),
        };
        Ray { origin, dir }
    }
}

/// One user action: pick a part, turn it, click.
#[derive(Clone, Debug)]
pub struct Step {
    /// Part id.
    pub part: &'static str,
    /// Colour id.
    pub color: &'static str,
    /// Turns before the click, as the keys `R` (Z), `T` (X) and `Y` pressed in order.
    pub turns: &'static str,
    /// Cursor.
    pub aim: Aim,
    /// Aim through the robot to its underside (the `U` key).
    pub under: bool,
    /// New device name, if the learner renames it.
    pub name: Option<&'static str>,
    /// New port, if the learner changes it.
    pub port: Option<Port>,
}

fn step(part: &'static str, color: &'static str, turns: &'static str, aim: Aim) -> Step {
    Step {
        part,
        color,
        turns,
        aim,
        under: false,
        name: None,
        port: None,
    }
}

fn named(mut s: Step, name: &'static str) -> Step {
    s.name = Some(name);
    s
}

fn below(mut s: Step) -> Step {
    s.under = true;
    s
}

fn on_port(mut s: Step, port: Port) -> Step {
    s.port = Some(port);
    s
}

/// Where the hub is aimed at on the Rover's plate, mm.
const HUB_X: f64 = -16.0;

/// Where the caster stack is aimed at, mm: 44 mm behind the axle. With the caster only 28 mm behind
/// it (the first layout) the wheels carried too little load and lost grip on the ramps; the plate
/// does not allow the stack any further back (decision 0015).
const CASTER_X: f64 = -40.0;

/// The Rover recipe: two medium motors, two 56 mm wheels, a caster, an ultrasonic sensor and a
/// colour sensor, on an 8x16 plate under a hub. The motors, caster and colour sensor hang under
/// the plate; the wheels sit outside it on 3-unit axles.
pub fn rover_steps() -> Vec<Step> {
    vec![
        step("plate_8x16", "dark_grey", "", Aim::Down(0.0, 0.0)),
        // The hub sits one stud behind the middle of the plate. It is by far the heaviest part, so
        // this puts the centre of mass behind the axle and the Rover does not tip over its wheels
        // when it brakes (decision 0015).
        named(step("hub", "white", "", Aim::Down(HUB_X, 0.0)), "hub"),
        on_port(
            named(
                step(
                    "sensor_ultrasonic",
                    "black",
                    "",
                    Aim::Down(24.0 + HUB_X, 0.0),
                ),
                "eye",
            ),
            Port::C,
        ),
        // Motors: the output face points outward (+Y on the left, -Y on the right).
        on_port(
            named(
                below(step(
                    "motor_medium",
                    "light_grey",
                    "",
                    Aim::Down(4.0, 37.25),
                )),
                "left",
            ),
            Port::A,
        ),
        on_port(
            named(
                below(step(
                    "motor_medium",
                    "light_grey",
                    "RR",
                    Aim::Down(4.0, -37.25),
                )),
                "right",
            ),
            Port::B,
        ),
        step("axle_3", "dark_grey", "R", Aim::FromPosY(4.0, -12.0)),
        step("axle_3", "dark_grey", "RRR", Aim::FromNegY(4.0, -12.0)),
        step("wheel_56", "white", "T", Aim::FromPosY(4.0, -12.0)),
        step("wheel_56", "white", "T", Aim::FromNegY(4.0, -12.0)),
        // Caster stack at the back: two 2x2 bricks lift the ball to the wheels' floor line.
        below(step("brick_2x2", "red", "", Aim::Down(CASTER_X, 0.0))),
        below(step("brick_2x2", "red", "", Aim::Down(CASTER_X, 0.0))),
        below(step(
            "caster_ball",
            "dark_grey",
            "",
            Aim::Down(CASTER_X, 0.0),
        )),
        on_port(
            named(
                below(step("sensor_color", "white", "", Aim::Down(24.0, 0.0))),
                "color",
            ),
            Port::D,
        ),
    ]
}

/// The Rover with a time-of-flight sensor next to the ultrasonic one (mission m05: the ultrasonic
/// sensor cannot see a wall turned more than 40 degrees away, the time-of-flight sensor can up to
/// 75 degrees).
pub fn rover_tof_steps() -> Vec<Step> {
    let mut steps = rover_steps();
    steps.push(on_port(
        named(
            step("sensor_tof", "black", "", Aim::Down(TOF_X, TOF_Y)),
            "tof",
        ),
        Port::E,
    ));
    steps
}

/// Where the time-of-flight sensor of [`rover_tof_steps`] is aimed at, mm.
const TOF_X: f64 = HUB_X - 8.0;
const TOF_Y: f64 = 0.0;

/// The Test chassis recipe: a hub on a plate, no other devices.
pub fn test_chassis_steps() -> Vec<Step> {
    vec![
        step("plate_6x8", "dark_grey", "R", Aim::Down(0.0, 0.0)),
        named(step("hub", "white", "", Aim::Down(0.0, 0.0)), "hub"),
    ]
}

/// Run a recipe through the builder commands (pick, turn, hover, click, rename).
pub fn run_steps(b: &mut Builder, steps: &[Step]) -> Result<(), BuildError> {
    for s in steps {
        b.start_place(s.part, Some(s.color))?;
        b.underneath = s.under;
        for key in s.turns.chars() {
            b.rotate_ghost(match key {
                'R' => TurnAxis::Z,
                'T' => TurnAxis::X,
                _ => TurnAxis::Y,
            });
        }
        b.hover(&s.aim.ray());
        let uids = b.click_place()?;
        if let Some(&uid) = uids.first() {
            if let Some(n) = s.name {
                b.set_device_name(uid, n)?;
            }
            if let Some(p) = s.port {
                b.set_device_port(uid, p)?;
            }
        }
    }
    b.cancel();
    Ok(())
}

/// Build a starter robot by id (`rover` or `test_chassis`).
pub fn build_starter(cat: &Catalog, id: &str) -> Result<Assembly, BuildError> {
    let (name, steps) = match id {
        "rover" => ("Rover", rover_steps()),
        "rover_tof" => ("Rover with ToF", rover_tof_steps()),
        "test_chassis" => ("Test chassis", test_chassis_steps()),
        other => return Err(BuildError::UnknownPart(other.into())),
    };
    let mut b = Builder::new(cat.clone());
    b.assembly.name = name.into();
    run_steps(&mut b, &steps)?;
    Ok(b.assembly.clone())
}

/// Starter robot ids and the file stem each is stored under.
pub const STARTERS: [(&str, &str); 3] = [
    ("rover", "rover"),
    ("rover_tof", "rover_tof"),
    ("test_chassis", "test_chassis"),
];

/// Move the cursor of a step `dx` mm along X.
fn shifted(mut s: Step, dx: f64) -> Step {
    s.aim = match s.aim {
        Aim::Down(x, y) => Aim::Down(x + dx, y),
        Aim::FromPosY(x, z) => Aim::FromPosY(x + dx, z),
        Aim::FromNegY(x, z) => Aim::FromNegY(x + dx, z),
    };
    s
}

/// The reference design for mission m12 (a 25 degree ramp): the Rover without the colour sensor
/// that hangs under its nose. The motors and wheels sit 24 mm farther forward on the plate (the
/// plate overhangs them by 36 mm instead of 60 mm, so its nose clears the foot of the ramp), the
/// hub and the ultrasonic sensor 34 mm farther forward (the centre of mass moves over the axle, so
/// the wheels keep their grip when the ramp shifts the weight backwards) and the caster stack
/// 8 mm forward (60 mm behind the axle). It is placed first: the underside aim cannot reach the
/// back of the plate once the hub is on it.
pub fn climber_steps() -> Vec<Step> {
    let mut steps: Vec<Step> = rover_steps()
        .into_iter()
        .filter(|s| s.name != Some("color"))
        .map(|s| match s.part {
            "hub" | "sensor_ultrasonic" => shifted(s, HUB_SHIFT),
            "motor_medium" | "axle_3" | "wheel_56" => shifted(s, AXLE_SHIFT),
            "brick_2x2" | "caster_ball" => shifted(s, CASTER_SHIFT),
            _ => s,
        })
        .collect();
    // The caster stack goes on first: with the hub already in place the cursor ray for the
    // underside lands on the hub, and the stack cannot be placed far behind it.
    let at = steps
        .iter()
        .position(|s| s.part == "brick_2x2")
        .unwrap_or(1);
    let caster: Vec<Step> = steps.drain(at..at + 3).collect();
    steps.splice(1..1, caster);
    steps
}

const HUB_SHIFT: f64 = 40.0;
const AXLE_SHIFT: f64 = 24.0;
const CASTER_SHIFT: f64 = 8.0;

/// Build the reference design for mission m12 (`climber`).
pub fn build_climber(cat: &Catalog) -> Result<Assembly, BuildError> {
    let mut b = Builder::new(cat.clone());
    b.assembly.name = "Climber".into();
    run_steps(&mut b, &climber_steps())?;
    Ok(b.assembly.clone())
}
