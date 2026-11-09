//! Hand-built assemblies for the tests and the accuracy suite: a four-bar linkage and gear pairs.
//!
//! They are built with the same builder commands as the starter robots where the snapping rules
//! allow it. Where they do not (a hinged holder at an exact hole), parts are placed so that chosen
//! connectors coincide, and the compiler derives the connections as usual.

use axl_build::builder::BuildError;
use axl_build::starter::{Aim, Step, run_steps};
use axl_build::{Assembly, Builder, Orient, PlacedPart, Port, TurnAxis};
use axl_parts::catalog::Catalog;
use axl_parts::types::{Axis, ColorId, ConnKind, PartId};

/// Errors from building a rig.
#[derive(Debug, thiserror::Error)]
pub enum RigError {
    /// A builder command was refused.
    #[error("{0}")]
    Build(#[from] BuildError),
    /// A part or connector the rig needs does not exist.
    #[error("rig: {0}")]
    Missing(String),
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

fn find_orient(x: [i32; 3], y: [i32; 3]) -> Option<Orient> {
    (0..24u8)
        .filter_map(Orient::from_index)
        .find(|o| o.rotate([1, 0, 0]) == x && o.rotate([0, 1, 0]) == y)
}

/// An orientation whose x axis maps to `x` and whose z axis maps to `z`.
fn find_orient_xz(x: [i32; 3], z: [i32; 3]) -> Option<Orient> {
    (0..24u8)
        .filter_map(Orient::from_index)
        .find(|o| o.rotate([1, 0, 0]) == x && o.rotate([0, 0, 1]) == z)
}

/// Place `part` so that its connector number `conn` (in sub-body 0) lies at `target`.
fn place_on(
    cat: &Catalog,
    asm: &mut Assembly,
    part: &str,
    orient: Orient,
    conn: usize,
    target: [i32; 3],
) -> Result<u32, RigError> {
    let def = cat
        .get(part)
        .ok_or_else(|| RigError::Missing(format!("part {part}")))?;
    let c = def.bodies[0]
        .connectors
        .get(conn)
        .ok_or_else(|| RigError::Missing(format!("{part} connector {conn}")))?;
    let off = orient.rotate(c.pos);
    let pos = [target[0] - off[0], target[1] - off[1], target[2] - off[2]];
    Ok(asm.push(PlacedPart {
        uid: 0,
        part: PartId(part.into()),
        pos,
        orient,
        color: def
            .colors
            .first()
            .cloned()
            .unwrap_or(ColorId("white".into())),
        device: None,
    }))
}

fn world_conn(cat: &Catalog, p: &PlacedPart, body: usize, conn: usize) -> Option<[i32; 3]> {
    let c = cat.get(&p.part.0)?.bodies.get(body)?.connectors.get(conn)?;
    let o = p.orient.rotate(c.pos);
    Some([p.pos[0] + o[0], p.pos[1] + o[1], p.pos[2] + o[2]])
}

/// Round-hole connector numbers of a part, in connector order.
fn hole_indices(cat: &Catalog, part: &str) -> Vec<usize> {
    cat.get(part)
        .map(|d| {
            d.bodies[0]
                .connectors
                .iter()
                .enumerate()
                .filter(|(_, c)| c.kind == ConnKind::RoundHole)
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default()
}

/// The Rover's plate and hub, built with the builder commands.
fn base_chassis(cat: &Catalog) -> Result<Builder, RigError> {
    let mut b = Builder::new(cat.clone());
    b.assembly.name = "Rig".into();
    run_steps(
        &mut b,
        &[
            step("plate_8x16", "dark_grey", "", Aim::Down(0.0, 0.0)),
            step("hub", "white", "", Aim::Down(0.0, 0.0)),
        ],
    )?;
    Ok(b)
}

/// A test chassis with a hole brick hung under its plate. Returns the assembly and the brick uid.
fn chassis_with_hole_brick(cat: &Catalog) -> Result<(Assembly, u32), RigError> {
    let mut b = base_chassis(cat)?;
    let mut s = step("hole_brick_8x1", "red", "", Aim::Down(0.0, 44.0));
    s.under = true;
    run_steps(&mut b, &[s])?;
    let uid = b
        .assembly
        .parts
        .iter()
        .find(|p| p.part.0 == "hole_brick_8x1")
        .map(|p| p.uid)
        .ok_or_else(|| RigError::Missing("hole brick".into()))?;
    Ok((b.assembly.clone(), uid))
}

/// A parallelogram four-bar linkage hanging from a hole brick under the test chassis.
///
/// Ground (the brick, rigid with the hub) is bar 1. Crank and rocker (5-hole beams) hang from two
/// of its holes on smooth pins, and a coupler beam joins their lower ends. Pins join layers 8 mm
/// apart. Four hinge joints, one of which closes the loop.
pub fn four_bar(cat: &Catalog) -> Result<Assembly, RigError> {
    let (mut asm, brick_uid) = chassis_with_hole_brick(cat)?;
    let brick = asm
        .get(brick_uid)
        .cloned()
        .ok_or_else(|| RigError::Missing("brick".into()))?;
    let holes = hole_indices(cat, "hole_brick_8x1");
    let pivot = |k: usize| world_conn(cat, &brick, 0, holes[k]);
    // Pivots four holes (32 mm) apart.
    let (p1, p2) = match (pivot(0), pivot(4)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err(RigError::Missing("brick holes".into())),
    };
    let layer = 80; // 8 mm in 0.1 mm steps
    let down = find_orient([0, 0, -1], [0, 1, 0])
        .ok_or_else(|| RigError::Missing("orientation".into()))?;
    let along = Orient::IDENTITY;
    let pin_o = Orient::IDENTITY.turned(TurnAxis::Z); // pin axis X -> Y
    let at = |p: [i32; 3], dy: i32, dz: i32| [p[0], p[1] + dy, p[2] + dz];

    // Beam connector 0 is its first hole.
    place_on(cat, &mut asm, "beam_5", down, 0, at(p1, layer, 0))?; // crank
    place_on(cat, &mut asm, "beam_5", down, 0, at(p2, layer, 0))?; // rocker
    place_on(cat, &mut asm, "beam_5", along, 0, at(p1, 2 * layer, -240))?; // coupler
    // Pins: shaft 0 at the lower layer, shaft 1 at the next one.
    place_on(cat, &mut asm, "pin_smooth", pin_o, 0, p1)?;
    place_on(cat, &mut asm, "pin_smooth", pin_o, 0, p2)?;
    place_on(cat, &mut asm, "pin_smooth", pin_o, 0, at(p1, layer, -240))?;
    place_on(cat, &mut asm, "pin_smooth", pin_o, 0, at(p2, layer, -240))?;
    Ok(asm)
}

fn name_port(s: &mut Step, name: &'static str, port: Port) {
    s.name = Some(name);
    s.port = Some(port);
}

/// Two medium motors side by side under the chassis, an axle on each, `teeth_a` on the first and
/// `teeth_b` on the second. The motors sit `(teeth_a + teeth_b) / 2` mm apart, so the gears mesh.
/// The motors are named `drive` and `idle`. `(teeth_a + teeth_b) / 2` must be a multiple of 8 and
/// at least 24.
pub fn gear_pair(cat: &Catalog, teeth_a: u32, teeth_b: u32) -> Result<Assembly, RigError> {
    let gear = |t: u32| -> Result<&'static str, RigError> {
        Ok(match t {
            8 => "gear_8",
            12 => "gear_12",
            16 => "gear_16",
            20 => "gear_20",
            24 => "gear_24",
            36 => "gear_36",
            40 => "gear_40",
            _ => return Err(RigError::Missing(format!("gear with {t} teeth"))),
        })
    };
    let d = ((teeth_a + teeth_b) / 2) as f64;
    let mut b = base_chassis(cat)?;
    let mut steps = Vec::new();
    for (i, (x, t)) in [(4.0, teeth_a), (4.0 + d, teeth_b)].into_iter().enumerate() {
        let mut motor = step("motor_medium", "light_grey", "", Aim::Down(x, 37.25));
        motor.under = true;
        name_port(
            &mut motor,
            if i == 0 { "drive" } else { "idle" },
            if i == 0 { Port::A } else { Port::B },
        );
        steps.push(motor);
        steps.push(step("axle_3", "dark_grey", "R", Aim::FromPosY(x, -12.0)));
        steps.push(step(gear(t)?, "white", "T", Aim::FromPosY(x, -12.0)));
    }
    run_steps(&mut b, &steps)?;
    Ok(b.assembly.clone())
}

/// Two gears whose axles are held by different parts: gear A turns in a hole brick fixed to the
/// chassis, gear B in a beam that is only pinned to it. Compiling it must fail
/// with the gear-parent error.
pub fn gear_parent_mismatch(cat: &Catalog) -> Result<Assembly, RigError> {
    let (mut asm, l1_uid) = chassis_with_hole_brick(cat)?;
    let l1 = asm
        .get(l1_uid)
        .cloned()
        .ok_or_else(|| RigError::Missing("brick".into()))?;
    let holes = hole_indices(cat, "hole_brick_8x1");
    let n = holes.len();
    let hole = |p: &PlacedPart, k: usize| world_conn(cat, p, 0, holes[k]);
    let layer = 80;
    let axle_o = Orient::IDENTITY.turned(TurnAxis::Z); // axle X -> Y
    let axle_back =
        find_orient([0, -1, 0], [1, 0, 0]).ok_or_else(|| RigError::Missing("orient".into()))?;
    let pin_o = axle_o;
    // Gear A on an axle through L1's last hole, running toward -Y.
    let a_hole = hole(&l1, n - 1).ok_or_else(|| RigError::Missing("hole".into()))?;
    place_on(cat, &mut asm, "axle_3", axle_back, 0, a_hole)?;
    let gear_o =
        find_orient_xz([0, 1, 0], [0, 1, 0]).or_else(|| find_orient_xz([1, 0, 0], [0, 1, 0]));
    let gear_o = gear_o.ok_or_else(|| RigError::Missing("gear orientation".into()))?;
    place_on(
        cat,
        &mut asm,
        "gear_8",
        gear_o,
        0,
        [a_hole[0], a_hole[1] - layer, a_hole[2]],
    )?;
    // B2: a 5-hole beam one layer behind L1, pinned to L1 at its second-to-last hole. Its fifth
    // hole is 24 mm beyond the axle of gear A.
    let shared = hole(&l1, n - 2).ok_or_else(|| RigError::Missing("hole".into()))?;
    place_on(cat, &mut asm, "pin_smooth", pin_o, 0, shared)?;
    let b2_uid = place_on(
        cat,
        &mut asm,
        "beam_5",
        Orient::IDENTITY,
        0,
        [shared[0], shared[1] + layer, shared[2]],
    )?;
    let b2 = asm
        .get(b2_uid)
        .cloned()
        .ok_or_else(|| RigError::Missing("beam".into()))?;
    let b_hole = world_conn(cat, &b2, 0, 4).ok_or_else(|| RigError::Missing("hole".into()))?;
    place_on(cat, &mut asm, "axle_3", axle_back, 0, b_hole)?;
    place_on(
        cat,
        &mut asm,
        "gear_40",
        gear_o,
        0,
        [b_hole[0], b_hole[1] - 2 * layer, b_hole[2]],
    )?;
    Ok(asm)
}

/// Re-export for callers that need the axis type of a connector.
pub fn axis_of(a: Axis) -> [f64; 3] {
    a.unit()
}
