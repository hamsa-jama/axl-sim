mod common;
use axl_compile::rigs;
use axl_compile::{CompileError, CompileOptions, PhysicsParams, compile};
use axl_mujoco::ActuatorId;
use axl_parts::catalog::Catalog;
use common::*;

fn world(
    model: &axl_compile::RobotModel,
    body: usize,
    local: [f64; 3],
    p: &axl_mujoco::Physics,
) -> [f64; 3] {
    let pose = p.body_pose(axl_mujoco::BodyId(body + 1));
    let r = rotate(pose.quat, local);
    [pose.pos[0] + r[0], pose.pos[1] + r[1], pose.pos[2] + r[2]]
        // `model` is only used to keep the signature honest about which body indices are meant.
        .map(|v| {
            let _ = model.bodies.len();
            v
        })
}

#[test]
fn four_bar_compiles_to_a_tree_and_one_loop() {
    let cat = Catalog::builtin();
    let asm = rigs::four_bar(&cat).expect("rig");
    let c = compile_asm(&asm);
    // Ground (with the hub), crank, rocker, coupler. Pins merged into their neighbours.
    assert_eq!(
        c.model.bodies.len(),
        4,
        "{:#?}",
        c.model.bodies.iter().map(|b| &b.name).collect::<Vec<_>>()
    );
    assert_eq!(c.model.loops.len(), 1);
    assert!(c.mjcf.contains("<connect"));
    assert_eq!(c.mjcf.matches("<connect").count(), 2);
    assert_eq!(c.mjcf.matches("type=\"hinge\"").count(), 3);
}

#[test]
fn four_bar_moves_and_the_loop_stays_closed() {
    let cat = Catalog::builtin();
    let asm = rigs::four_bar(&cat).expect("rig");
    let c = compile_asm(&asm);
    let mut p = physics(&c);
    // The two joints that hang from the ground link: the crank and the rocker.
    let hinges: Vec<&str> = c
        .model
        .bodies
        .iter()
        .filter_map(|b| match &b.joint {
            Some(axl_compile::model::ModelJoint::Hinge { name, .. }) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(hinges.len(), 3);
    let crank = p.joint_id(hinges[0]).unwrap();
    let rocker = p.joint_id(hinges[1]).unwrap();
    p.set_joint_pos(crank, 0.35);
    p.forward();
    for _ in 0..600 {
        p.step();
    }
    // A parallelogram: crank and rocker turn together, which is only possible when the loop is
    // closed. Without the connect constraints the rocker would stay where it was.
    let (qc, qr) = (p.joint_pos(crank), p.joint_pos(rocker));
    assert!(qc.abs() > 0.05, "the linkage must have moved: crank {qc}");
    assert!(
        (qc - qr).abs() < 0.02,
        "crank {qc} and rocker {qr} must turn together"
    );
    // The two bodies of the closing joint agree on the anchor points.
    for l in &c.model.loops {
        for t in [0.0, 0.008] {
            let a = [
                l.anchor[0] + l.axis[0] * t,
                l.anchor[1] + l.axis[1] * t,
                l.anchor[2] + l.axis[2] * t,
            ];
            let o1 = c.model.bodies[l.body1].origin;
            let o2 = c.model.bodies[l.body2].origin;
            let b1 = p.body_id(&c.model.bodies[l.body1].name).unwrap().0 - 1;
            let b2 = p.body_id(&c.model.bodies[l.body2].name).unwrap().0 - 1;
            let w1 = world(&c.model, b1, [a[0] - o1[0], a[1] - o1[1], a[2] - o1[2]], &p);
            let w2 = world(&c.model, b2, [a[0] - o2[0], a[1] - o2[1], a[2] - o2[2]], &p);
            let d = ((w1[0] - w2[0]).powi(2) + (w1[1] - w2[1]).powi(2) + (w1[2] - w2[2]).powi(2))
                .sqrt();
            assert!(d < 0.001, "loop gap {d} m");
        }
    }
}

#[test]
fn gears_on_axles_of_different_parts_are_refused_with_the_spec_message() {
    let cat = Catalog::builtin();
    let asm = rigs::gear_parent_mismatch(&cat).expect("rig");
    let err = compile(
        &cat,
        &asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(err, CompileError::GearParents { .. }), "{err:?}");
    assert_eq!(
        err.to_string(),
        "Gears must turn on axles held by the same part"
    );
}

#[test]
fn meshed_gears_become_a_polycoef_coupling() {
    let cat = Catalog::builtin();
    let asm = rigs::gear_pair(&cat, 8, 40).expect("rig");
    let c = compile_asm(&asm);
    assert_eq!(c.model.gears.len(), 1, "{}", c.mjcf);
    assert!(
        (c.model.gears[0].ratio + 5.0).abs() < 1e-12,
        "{}",
        c.model.gears[0].ratio
    );
    assert!(c.mjcf.contains("polycoef=\"0 -5 0 0 0\""), "{}", c.mjcf);
    // The driven motor's hinge carries the mesh friction on top of its gearbox loss.
    let p = physics(&c);
    assert_eq!(p.actuator_count(), 2);
    let _ = ActuatorId(0);
}
