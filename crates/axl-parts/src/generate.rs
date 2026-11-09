//! The built-in catalog: all 75 parts, generated from their parameters.
//!
//! `cargo xtask gen-parts` writes these to `assets/parts/*.ron`; a test checks that the files
//! on disk still match, so the generator and the files cannot drift apart.

use crate::build::{block_spec, solid_volume_mm3};
use crate::devices::{device_studs, housing_box};
use crate::types::*;

/// ABS density, kg/m3 (SPEC.md, "Mass").
pub const ABS_DENSITY: f64 = 1040.0;

fn colors(ids: &[&str]) -> Vec<ColorId> {
    ids.iter().map(|s| ColorId((*s).into())).collect()
}

const ALL: [&str; 9] = [
    "red",
    "blue",
    "yellow",
    "green",
    "orange",
    "white",
    "light_grey",
    "dark_grey",
    "black",
];

fn all_from(first: &str) -> Vec<ColorId> {
    let mut v = vec![first];
    v.extend(ALL.iter().copied().filter(|c| *c != first));
    colors(&v)
}

/// mm to the 0.1 mm integer grid.
fn t(mm: f64) -> i32 {
    (mm * 10.0).round() as i32
}

fn m(mm: f64) -> f64 {
    (mm * 1e-3 * 1e9).round() / 1e9
}

fn conn(kind: ConnKind, x: f64, y: f64, z: f64, axis: Axis) -> Connector {
    Connector {
        kind,
        pos: [t(x), t(y), t(z)],
        axis,
    }
}

fn bx(c: [f64; 3], h: [f64; 3]) -> CollisionPrim {
    CollisionPrim::Box {
        center: c.map(m),
        half: h.map(m),
    }
}

fn computed(shape: &ShapeGen) -> (f64, MassSource) {
    let v = solid_volume_mm3(shape).expect("computed-mass shapes have an analytic volume");
    // Round to 12 decimals so the files are stable text.
    let kg = v * 1e-9 * ABS_DENSITY;
    let kg = (kg * 1e12).round() / 1e12;
    (
        kg,
        MassSource::Computed {
            density_kg_m3: ABS_DENSITY,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn part(
    id: &str,
    name: &str,
    category: Category,
    shape: ShapeGen,
    bodies: Vec<SubBody>,
    colors: Vec<ColorId>,
) -> PartDef {
    PartDef {
        version: 1,
        id: PartId(id.into()),
        name: name.into(),
        category,
        shape,
        bodies,
        internal_joint: None,
        material: MaterialKind::Abs,
        colors,
        device: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn block_part(
    cat: Category,
    id: String,
    name: String,
    nx: u32,
    ny: u32,
    plates: u32,
    studs: bool,
    holes: bool,
    first: &str,
) -> PartDef {
    let shape = ShapeGen::Block {
        nx,
        ny,
        plates,
        studs,
        holes,
    };
    let spec = block_spec(nx, ny, plates, studs, holes);
    let (l, w, h) = (spec.len_x(), spec.len_y(), spec.height);
    let mut cs = Vec::new();
    if studs {
        for (x, y) in spec.stud_positions() {
            cs.push(conn(ConnKind::Stud, x, y, h, Axis::PosZ));
        }
    }
    for (x, y) in spec.stud_positions() {
        cs.push(conn(ConnKind::AntiStud, x, y, 0.0, Axis::NegZ));
    }
    for &(x, z) in &spec.holes_y {
        cs.push(conn(ConnKind::RoundHole, x, 0.0, z, Axis::PosY));
    }
    let (mass, src) = computed(&shape);
    part(
        &id,
        &name,
        cat,
        shape,
        vec![SubBody {
            mass_kg: mass,
            mass_source: src,
            collision: vec![bx([0.0, 0.0, h / 2.0], [l / 2.0, w / 2.0, h / 2.0])],
            connectors: cs,
        }],
        all_from(first),
    )
}

fn beam_part(id: String, name: String, shape: BeamShape) -> PartDef {
    use crate::shapes::*;
    let sg = ShapeGen::Beam { shape };
    let cs: Vec<Connector> = beam_hole_centres(shape)
        .into_iter()
        .map(|(x, z)| conn(ConnKind::RoundHole, x, 0.0, z, Axis::PosY))
        .collect();
    let r = BEAM_W / 2.0;
    let coll = match shape {
        BeamShape::Straight { holes } => {
            let x = (holes - 1) as f64 * UNIT;
            vec![bx([x / 2.0, 0.0, 0.0], [x / 2.0 + r, r, r])]
        }
        BeamShape::L { long, short } => {
            let (x, z) = ((long - 1) as f64 * UNIT, (short - 1) as f64 * UNIT);
            vec![
                bx([x / 2.0, 0.0, 0.0], [x / 2.0 + r, r, r]),
                bx([x, 0.0, z / 2.0], [r, r, z / 2.0 + r]),
            ]
        }
        BeamShape::Frame { w, h } => {
            let (x, z) = ((w - 1) as f64 * UNIT, (h - 1) as f64 * UNIT);
            vec![
                bx([x / 2.0, 0.0, 0.0], [x / 2.0 + r, r, r]),
                bx([x / 2.0, 0.0, z], [x / 2.0 + r, r, r]),
                bx([0.0, 0.0, z / 2.0], [r, r, z / 2.0 + r]),
                bx([x, 0.0, z / 2.0], [r, r, z / 2.0 + r]),
            ]
        }
    };
    let (mass, src) = computed(&sg);
    part(
        &id,
        &name,
        Category::Beam,
        sg,
        vec![SubBody {
            mass_kg: mass,
            mass_source: src,
            collision: coll,
            connectors: cs,
        }],
        all_from("blue"),
    )
}

fn axle_part(units: u32) -> PartDef {
    let sg = ShapeGen::Axle { units };
    let cs = (0..units)
        .map(|k| {
            conn(
                ConnKind::AxleShaft { len: units as u8 },
                k as f64 * 8.0,
                0.0,
                0.0,
                Axis::PosX,
            )
        })
        .collect();
    let coll = vec![CollisionPrim::Cylinder {
        center: [m((units as f64 - 1.0) * 4.0), 0.0, 0.0],
        axis: Axis::PosX,
        radius: m(2.4),
        half_len: m(units as f64 * 4.0),
    }];
    let (mass, src) = computed(&sg);
    part(
        &format!("axle_{units}"),
        &format!("Axle {units} ({} mm)", units * 8),
        Category::Axle,
        sg,
        vec![SubBody {
            mass_kg: mass,
            mass_source: src,
            collision: coll,
            connectors: cs,
        }],
        all_from("dark_grey"),
    )
}

fn pin_part(id: &str, name: &str, units: u32, kind: PinKind) -> PartDef {
    let sg = ShapeGen::Pin { units, kind };
    let cs: Vec<Connector> = (0..units)
        .map(|k| {
            let kind_c = match kind {
                PinKind::Friction => ConnKind::PinShaft { friction: true },
                PinKind::Smooth => ConnKind::PinShaft { friction: false },
                PinKind::AxlePin if k == 0 => ConnKind::PinShaft { friction: false },
                PinKind::AxlePin => ConnKind::AxleShaft { len: 1 },
            };
            conn(kind_c, k as f64 * 8.0, 0.0, 0.0, Axis::PosX)
        })
        .collect();
    let coll = vec![CollisionPrim::Cylinder {
        center: [m((units as f64 - 1.0) * 4.0), 0.0, 0.0],
        axis: Axis::PosX,
        radius: m(2.4),
        half_len: m(units as f64 * 4.0),
    }];
    let (mass, src) = computed(&sg);
    part(
        id,
        name,
        Category::Pin,
        sg,
        vec![SubBody {
            mass_kg: mass,
            mass_source: src,
            collision: coll,
            connectors: cs,
        }],
        all_from("dark_grey"),
    )
}

fn gear_part(teeth: u32) -> PartDef {
    let sg = ShapeGen::Gear {
        teeth,
        thickness: 0.0032,
    };
    let (mass, src) = computed(&sg);
    let coll = vec![CollisionPrim::Cylinder {
        center: [0.0; 3],
        axis: Axis::PosZ,
        radius: m(teeth as f64 / 2.0),
        half_len: m(1.6),
    }];
    part(
        &format!("gear_{teeth}"),
        &format!("Gear {teeth}T"),
        Category::Gear,
        sg,
        vec![SubBody {
            mass_kg: mass,
            mass_source: src,
            collision: coll,
            connectors: vec![conn(ConnKind::AxleHole, 0.0, 0.0, 0.0, Axis::PosZ)],
        }],
        all_from("yellow"),
    )
}

fn wheel_part(d_mm: u32, w_mm: f64, mass: f64) -> PartDef {
    let sg = ShapeGen::Wheel {
        diameter: m(d_mm as f64),
        width: m(w_mm),
    };
    let coll = vec![CollisionPrim::Cylinder {
        center: [0.0; 3],
        axis: Axis::PosZ,
        radius: m(d_mm as f64 / 2.0),
        half_len: m(w_mm / 2.0),
    }];
    let mut p = part(
        &format!("wheel_{d_mm}"),
        &format!("Wheel {d_mm} mm"),
        Category::Wheel,
        sg,
        vec![SubBody {
            mass_kg: mass,
            mass_source: MassSource::Placeholder,
            collision: coll,
            connectors: vec![conn(ConnKind::AxleHole, 0.0, 0.0, 0.0, Axis::PosZ)],
        }],
        all_from("white"),
    );
    p.material = MaterialKind::Abs;
    p
}

fn device_part(
    id: &str,
    name: &str,
    cat: Category,
    model: DeviceModel,
    mass: f64,
    col: &str,
) -> PartDef {
    let (nx, ny, top, c) = device_studs(model);
    let (bc, bh) = housing_box(model);
    let mut cs = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            let x = c[0] + (i as f64 - (nx as f64 - 1.0) / 2.0) * 8.0;
            let y = c[1] + (j as f64 - (ny as f64 - 1.0) / 2.0) * 8.0;
            // Colour and distance sensors that look down or forward still carry studs on top.
            if model != DeviceModel::Color || (i, j) != (9, 9) {
                cs.push(conn(ConnKind::Stud, x, y, top, Axis::PosZ));
            }
        }
    }
    let has_cavity = matches!(
        model,
        DeviceModel::Hub | DeviceModel::Ultrasonic | DeviceModel::TimeOfFlight | DeviceModel::Touch
    );
    if has_cavity {
        for i in 0..nx {
            for j in 0..ny {
                let x = c[0] + (i as f64 - (nx as f64 - 1.0) / 2.0) * 8.0;
                let y = c[1] + (j as f64 - (ny as f64 - 1.0) / 2.0) * 8.0;
                cs.push(conn(ConnKind::AntiStud, x, y, 0.0, Axis::NegZ));
            }
        }
    }
    let mut p = part(
        id,
        name,
        cat,
        ShapeGen::Device { model },
        vec![],
        colors(&[col, "white", "light_grey", "dark_grey", "black"]),
    );
    let mut body = SubBody {
        mass_kg: mass,
        mass_source: MassSource::Placeholder,
        collision: vec![bx(bc, bh)],
        connectors: cs,
    };
    let motor = matches!(
        model,
        DeviceModel::MotorMedium | DeviceModel::MotorLarge | DeviceModel::Servo
    );
    if motor {
        // Rotor takes 15 % of the placeholder mass; the output cross hole faces +Y.
        let rotor_mass = (mass * 0.15 * 1e6).round() / 1e6;
        body.mass_kg = ((mass - rotor_mass) * 1e6).round() / 1e6;
        let rotor = SubBody {
            mass_kg: rotor_mass,
            mass_source: MassSource::Placeholder,
            collision: vec![CollisionPrim::Cylinder {
                center: [0.0, m(1.25), 0.0],
                axis: Axis::PosY,
                radius: m(6.0),
                half_len: m(1.25),
            }],
            connectors: vec![conn(ConnKind::AxleHole, 0.0, 0.0, 0.0, Axis::PosY)],
        };
        p.bodies = vec![body, rotor];
        let range = (model == DeviceModel::Servo)
            .then_some([-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2]);
        p.internal_joint = Some(InternalJoint {
            axis: Axis::PosY,
            pos: [0, 0, 0],
            range_rad: range,
        });
    } else {
        p.bodies = vec![body];
    }
    let site = match model {
        DeviceModel::Hub => Some(SiteDef {
            pos: [0, 0, 112],
            axis: Axis::PosX,
        }),
        DeviceModel::Ultrasonic => Some(SiteDef {
            pos: [t(7.9 + 3.3), 0, t(9.6)],
            axis: Axis::PosX,
        }),
        DeviceModel::TimeOfFlight => Some(SiteDef {
            pos: [t(7.9 + 0.6), 0, t(6.4)],
            axis: Axis::PosX,
        }),
        DeviceModel::Color => Some(SiteDef {
            pos: [0, 0, 0],
            axis: Axis::NegZ,
        }),
        DeviceModel::Touch => Some(SiteDef {
            pos: [t(7.9 + 5.0), 0, t(4.8)],
            axis: Axis::PosX,
        }),
        _ => None,
    };
    p.device = Some(DeviceDef { kind: model, site });
    p
}

fn caster_part() -> PartDef {
    let sg = ShapeGen::Caster {
        ball_diameter: 0.016,
    };
    let mut cs = Vec::new();
    for (x, y) in [(-4.0, -4.0), (4.0, -4.0), (-4.0, 4.0), (4.0, 4.0)] {
        cs.push(conn(ConnKind::Stud, x, y, 20.8, Axis::PosZ));
    }
    let coll = vec![
        CollisionPrim::Sphere {
            center: [0.0, 0.0, m(8.0)],
            radius: m(8.0),
        },
        bx([0.0, 0.0, 8.0 + 6.4], [7.9, 7.9, 6.4]),
    ];
    part(
        "caster_ball",
        "Ball caster",
        Category::Caster,
        sg,
        vec![SubBody {
            mass_kg: 0.02,
            mass_source: MassSource::Placeholder,
            collision: coll,
            connectors: cs,
        }],
        colors(&["dark_grey", "black", "light_grey", "white"]),
    )
}

/// Every part of the version 1 catalog, sorted by id.
pub fn builtin_parts() -> Vec<PartDef> {
    use Category::*;
    let mut v: Vec<PartDef> = Vec::new();
    let mut blk = |cat: Category,
                   prefix: &str,
                   label: &str,
                   nx: u32,
                   ny: u32,
                   plates: u32,
                   studs: bool,
                   holes: bool,
                   first: &str| {
        v.push(block_part(
            cat,
            format!("{prefix}_{nx}x{ny}"),
            format!("{label} {nx}\u{d7}{ny}"),
            nx,
            ny,
            plates,
            studs,
            holes,
            first,
        ));
    };
    // Bricks: 9 listed plus 1x3 and 2x3.
    for (a, b) in [
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (1, 6),
        (1, 8),
        (2, 2),
        (2, 3),
        (2, 4),
        (2, 6),
        (2, 8),
    ] {
        blk(Brick, "brick", "Brick", a, b, 3, true, false, "red");
    }
    // Plates: 8 listed plus 1x1, 1x3, 1x6 and 6x8.
    for (a, b) in [
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (1, 6),
        (1, 8),
        (2, 2),
        (2, 4),
        (2, 8),
        (4, 8),
        (6, 8),
        (8, 16),
    ] {
        blk(Plate, "plate", "Plate", a, b, 1, true, false, "dark_grey");
    }
    // Tiles: 3 listed plus 1x1 and 1x4.
    for (a, b) in [(1, 1), (1, 2), (1, 4), (2, 2), (2, 4)] {
        blk(Tile, "tile", "Tile", a, b, 1, false, false, "white");
    }
    // Bricks with holes (holes along the long side): 4 listed plus 1x2.
    for n in [2, 4, 6, 8, 12] {
        blk(
            HoleBrick,
            "hole_brick",
            "Hole brick",
            n,
            1,
            3,
            true,
            true,
            "dark_grey",
        );
    }
    for n in [3, 5, 7, 9, 11, 13, 15] {
        v.push(beam_part(
            format!("beam_{n}"),
            format!("Beam {n}"),
            BeamShape::Straight { holes: n },
        ));
    }
    v.push(beam_part(
        "beam_l_3x5".into(),
        "L-beam 3\u{d7}5".into(),
        BeamShape::L { long: 5, short: 3 },
    ));
    v.push(beam_part(
        "beam_frame_5x7".into(),
        "Frame 5\u{d7}7".into(),
        BeamShape::Frame { w: 7, h: 5 },
    ));
    for n in [2, 3, 4, 5, 6, 7, 8, 10, 12] {
        v.push(axle_part(n));
    }
    v.push(pin_part(
        "pin_friction",
        "Friction pin",
        2,
        PinKind::Friction,
    ));
    v.push(pin_part("pin_smooth", "Smooth pin", 2, PinKind::Smooth));
    v.push(pin_part(
        "pin_friction_long",
        "Long friction pin",
        3,
        PinKind::Friction,
    ));
    v.push(pin_part("pin_axle", "Axle pin", 2, PinKind::AxlePin));
    for n in [8, 12, 16, 20, 24, 36, 40] {
        v.push(gear_part(n));
    }
    v.push(wheel_part(30, 8.0, 0.006));
    v.push(wheel_part(43, 12.0, 0.012));
    v.push(wheel_part(56, 14.0, 0.020));
    v.push(wheel_part(81, 18.0, 0.045));
    v.push(caster_part());
    v.push(device_part(
        "hub",
        "Controller hub",
        Hub,
        DeviceModel::Hub,
        0.150,
        "white",
    ));
    v.push(device_part(
        "motor_medium",
        "Medium motor",
        Motor,
        DeviceModel::MotorMedium,
        0.035,
        "light_grey",
    ));
    v.push(device_part(
        "motor_large",
        "Large motor",
        Motor,
        DeviceModel::MotorLarge,
        0.080,
        "light_grey",
    ));
    v.push(device_part(
        "servo_180",
        "Servo 180\u{b0}",
        Motor,
        DeviceModel::Servo,
        0.040,
        "light_grey",
    ));
    v.push(device_part(
        "sensor_ultrasonic",
        "Ultrasonic sensor",
        Sensor,
        DeviceModel::Ultrasonic,
        0.015,
        "dark_grey",
    ));
    v.push(device_part(
        "sensor_tof",
        "Time-of-flight sensor",
        Sensor,
        DeviceModel::TimeOfFlight,
        0.010,
        "dark_grey",
    ));
    v.push(device_part(
        "sensor_color",
        "Colour sensor",
        Sensor,
        DeviceModel::Color,
        0.012,
        "dark_grey",
    ));
    v.push(device_part(
        "sensor_touch",
        "Touch sensor",
        Sensor,
        DeviceModel::Touch,
        0.010,
        "dark_grey",
    ));
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_75_unique_parts() {
        let p = builtin_parts();
        assert_eq!(p.len(), 75);
        let mut ids: Vec<&str> = p.iter().map(|d| d.id.0.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 75);
    }

    #[test]
    fn family_counts_match_the_spec_or_the_documented_extras() {
        let p = builtin_parts();
        let n = |c: Category| p.iter().filter(|d| d.category == c).count();
        // Spec counts: 9 bricks, 8 plates, 3 tiles, 4 hole bricks (+2, +4, +2, +1 extras, decision 0006).
        assert_eq!(
            (
                n(Category::Brick),
                n(Category::Plate),
                n(Category::Tile),
                n(Category::HoleBrick)
            ),
            (11, 12, 5, 5)
        );
        assert_eq!(
            (
                n(Category::Beam),
                n(Category::Axle),
                n(Category::Pin),
                n(Category::Gear)
            ),
            (9, 9, 4, 7)
        );
        assert_eq!(
            (
                n(Category::Wheel),
                n(Category::Caster),
                n(Category::Hub),
                n(Category::Motor),
                n(Category::Sensor)
            ),
            (4, 1, 1, 3, 4)
        );
    }

    #[test]
    fn every_mass_is_positive_and_computed_ones_use_abs_density() {
        for d in builtin_parts() {
            for b in &d.bodies {
                assert!(b.mass_kg > 0.0, "{}", d.id.0);
                if let MassSource::Computed { density_kg_m3 } = b.mass_source {
                    assert_eq!(density_kg_m3, 1040.0);
                }
            }
        }
    }
}
