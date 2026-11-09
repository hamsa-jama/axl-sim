mod common;
use axl_mujoco::{ActuatorId, BodyId, JointId, SensorId, SiteId};
use common::*;

#[test]
fn rover_mjcf_snapshot() {
    insta::assert_snapshot!(compile_asm(&starter("rover")).mjcf);
}

#[test]
fn test_chassis_mjcf_snapshot() {
    insta::assert_snapshot!(compile_asm(&starter("test_chassis")).mjcf);
}

#[test]
fn rover_loads_and_ids_match_names() {
    let c = compile_asm(&starter("rover"));
    let p = physics(&c);
    // Every id in the device map equals the id MuJoCo gives the same name.
    for d in &c.devices.devices {
        if let Some(j) = &d.joint {
            assert_eq!(p.joint_id(&j.name).unwrap(), JointId(j.id), "{}", j.name);
        }
        if let Some(a) = &d.actuator {
            assert_eq!(p.actuator_id(&a.name).unwrap(), ActuatorId(a.id));
        }
        if let Some(s) = &d.site {
            assert_eq!(p.site_id(&s.name).unwrap(), SiteId(s.id), "{}", s.name);
        }
        for s in &d.sensors {
            assert_eq!(p.sensor_id(&s.sensor.name).unwrap(), SensorId(s.sensor.id));
        }
    }
    for b in &c.bodies.bodies {
        assert_eq!(p.body_id(&b.name).unwrap(), BodyId(b.id), "{}", b.name);
    }
    assert_eq!(p.body_count(), c.bodies.bodies.len() + 1);
    assert_eq!(p.actuator_count(), 2);
}

#[test]
fn rover_motor_ids_and_names() {
    let c = compile_asm(&starter("rover"));
    for n in ["left", "right"] {
        let d = c.devices.get(n).expect("motor");
        assert!(d.joint.is_some() && d.actuator.is_some() && d.motor.is_some());
    }
    assert!(c.devices.get("hub").unwrap().site.as_ref().unwrap().name == "imu");
    assert!(c.devices.get("eye").unwrap().site.is_some());
}

#[test]
fn rover_motor_forward_for_equal_voltages() {
    // Mirrored motors turn their wheels the same way for the same voltage (decision 0010):
    // equal positive voltages drive the Rover forward (+X).
    let c = compile_asm(&starter("rover"));
    let mut p = physics(&c);
    let l = c.devices.get("left").unwrap().actuator.as_ref().unwrap().id;
    let r = c
        .devices
        .get("right")
        .unwrap()
        .actuator
        .as_ref()
        .unwrap()
        .id;
    let root = p.body_id("root").unwrap();
    for _ in 0..1000 {
        p.set_ctrl(ActuatorId(l), 5.0);
        p.set_ctrl(ActuatorId(r), 5.0);
        p.step();
    }
    let pose = p.body_pose(root);
    assert!(pose.pos[0] > 0.05, "moved forward: {:?}", pose.pos);
    assert!(pose.pos[1].abs() < 0.02, "drifted sideways: {:?}", pose.pos);
}

#[test]
fn export_loads_back() {
    let c = compile_asm(&starter("rover"));
    let dir = std::env::temp_dir().join(format!("axl-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rover.xml");
    axl_compile::export_mjcf(&c, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, c.mjcf);
    // Load from the file itself, as a desktop MuJoCo would.
    let p = axl_mujoco::Physics::from_mjcf(&text).expect("exported file loads");
    assert!(p.body_count() > 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lowest_point_starts_half_a_millimetre_above_the_floor() {
    let c = compile_asm(&starter("rover"));
    let mut p = physics(&c);
    p.forward();
    let root = p.body_id("root").unwrap();
    let z = p.body_pose(root).pos[2];
    // Root origin is the hub origin; the wheels and ball are the lowest points.
    let low = c.model.min_z;
    let hub_z = c.model.bodies[0].origin[2];
    assert!((z - (hub_z - low + 0.0005)).abs() < 1e-9);
    // And the wheel geom bottoms are at 0.5 mm.
    assert!(c.mjcf.contains("name=\"floor\""));
}

#[test]
fn mass_is_conserved_by_the_split() {
    let c = compile_asm(&starter("rover"));
    let total: f64 = c
        .model
        .bodies
        .iter()
        .flat_map(|b| &b.geoms)
        .map(|g| g.mass)
        .sum();
    let cat = axl_parts::catalog::Catalog::builtin();
    let asm = starter("rover");
    let expect: f64 = asm
        .parts
        .iter()
        .map(|p| {
            cat.get(&p.part.0)
                .unwrap()
                .bodies
                .iter()
                .map(|b| b.mass_kg)
                .sum::<f64>()
        })
        .sum();
    assert!((total - expect).abs() < 1e-9, "{total} vs {expect}");
}
