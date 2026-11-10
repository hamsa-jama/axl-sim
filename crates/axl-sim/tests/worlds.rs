//! The shipped worlds and objects inside MuJoCo, with the Rover.

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, Compiled, PhysicsParams, compile};
use axl_mujoco::{BodyId, GroupMask};
use axl_parts::catalog::Catalog;
use axl_sim::SimCore;
use axl_world::{ObjectPlacement, WorldFriction, build_world, builtin_objects, builtin_worlds};

fn friction() -> WorldFriction {
    let p = PhysicsParams::builtin();
    WorldFriction {
        wall: p.friction.wall,
        plastic: p.friction.abs,
        rubber: p.friction.rubber_sliding,
    }
}

fn rover_in(
    def: &axl_world::WorldDef,
    variant: Option<&str>,
    seed: u64,
) -> (Compiled, axl_world::BuiltWorld) {
    let built = build_world(def, variant, seed, &builtin_objects().unwrap(), &friction()).unwrap();
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, "rover").unwrap();
    let c = compile(
        &cat,
        &asm,
        &PhysicsParams::builtin(),
        &CompileOptions {
            spawn_xy: [built.spawn.x, built.spawn.y],
            spawn_yaw: built.spawn.yaw,
            extra_worldbody: built.worldbody.clone(),
        },
    )
    .unwrap();
    (c, built)
}

#[test]
fn every_world_loads_with_the_rover_and_runs_a_second() {
    for (id, def) in builtin_worlds().unwrap() {
        let mut variants: Vec<Option<String>> = vec![None];
        variants.extend(def.variants.iter().map(|v| Some(v.id.clone())));
        for v in variants {
            for seed in [1, 2, 3] {
                let (c, _) = rover_in(&def, v.as_deref(), seed);
                let mut core = SimCore::new(&c, &[]).unwrap_or_else(|e| panic!("{id}/{v:?}: {e}"));
                for _ in 0..1000 {
                    assert!(core.tick(), "{id}/{v:?} seed {seed} became unstable");
                }
            }
        }
    }
}

#[test]
fn objects_rest_on_the_floor_at_half_their_height() {
    let mut def = builtin_worlds().unwrap().remove("open_grid").unwrap();
    let objects = builtin_objects().unwrap();
    for (i, id) in objects.keys().enumerate() {
        def.objects.push(ObjectPlacement {
            object: id.clone(),
            xy: [0.3 + 0.15 * i as f64, 0.5],
            yaw: 0.3,
        });
    }
    let (c, built) = rover_in(&def, None, 1);
    let mut core = SimCore::new(&c, &[]).unwrap();
    for _ in 0..1500 {
        assert!(core.tick());
    }
    for (name, (id, o)) in built.object_bodies.iter().zip(objects.iter()) {
        let body = core
            .physics()
            .body_id(name)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .0;
        let z = core.physics().body_pose(BodyId(body)).pos[2];
        assert!(
            (z - o.height() / 2.0).abs() < 0.002,
            "{id} rests at z = {z}, expected {}",
            o.height() / 2.0
        );
        let m = core.physics().body_mass(BodyId(body));
        assert!((m - o.mass).abs() < 1e-9, "{id}: mass {m}");
    }
}

#[test]
fn the_ramps_rise_at_their_angles() {
    let def = builtin_worlds().unwrap().remove("ramp_lab").unwrap();
    let (c, _) = rover_in(&def, Some("ramp_15"), 1);
    let mut core = SimCore::new(&c, &[]).unwrap();
    core.tick();
    for (k, deg) in [10.0_f64, 15.0, 20.0, 25.0].into_iter().enumerate() {
        let y = (k as f64 - 1.5) * 0.6;
        let th = deg.to_radians();
        // 0.5 m up the slope from the bottom edge at x = 0.3.
        let x = 0.3 + 0.5 * th.cos();
        let hit = core
            .physics_mut()
            .ray([x, y, 2.0], [0.0, 0.0, -1.0], GroupMask(0b1))
            .expect("a ramp under the ray");
        let want = 0.5 * th.sin();
        assert!(
            (hit.point[2] - want).abs() < 0.003,
            "{deg} deg: z {} want {want}",
            hit.point[2]
        );
    }
}
