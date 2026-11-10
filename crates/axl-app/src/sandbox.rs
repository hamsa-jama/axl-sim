//! The sandbox: a starter robot driven by fixed motor voltages on the sim thread, drawn through
//! the body map. This is the M5 stand-in for the Simulate screen.

use std::collections::BTreeMap;
use std::sync::Mutex;

use axl_build::starter::build_starter;
use axl_compile::Compiled;
use axl_compile::{CompileOptions, PhysicsParams, compile};
use axl_missions::MissionDef;
use axl_parts::catalog::Catalog;
use axl_render::bevy::prelude::*;
use axl_render::camera_rig::CameraHome;
use axl_render::sim_view::SandboxScene;
use axl_render::world_view::{
    MatImage, ObjectDraw, ObjectKind, StaticDraw, StaticKind, StaticMaterial, WorldScene, ZoneDraw,
    ZoneShape,
};
use axl_sim::Sandbox;
use axl_sim::mission::robot_assembly;

/// The running sim thread.
#[derive(Resource)]
pub struct SandboxSim(pub Mutex<Sandbox>);

/// Parse `name=volts,name=volts`.
pub fn parse_volts(s: &str) -> Result<Vec<(String, f64)>, String> {
    s.split(',')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let (n, v) = p
                .split_once('=')
                .ok_or_else(|| format!("{p:?} is not name=volts"))?;
            let v: f64 = v
                .trim()
                .parse()
                .map_err(|_| format!("{v:?} is not a number"))?;
            Ok((n.trim().to_string(), v))
        })
        .collect()
}

/// A compiled starter robot and what the renderer needs to draw it.
fn build_robot(starter: &str) -> Result<(Compiled, SandboxScene), String> {
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, starter).map_err(|e| e.to_string())?;
    let compiled = compile(
        &cat,
        &asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let parts: BTreeMap<u32, (String, String)> = asm
        .parts
        .iter()
        .map(|p| (p.uid, (p.part.0.clone(), p.color.0.clone())))
        .collect();
    let o = compiled
        .model
        .bodies
        .first()
        .map(|b| b.origin)
        .unwrap_or([0.0; 3]);
    let scene = SandboxScene {
        bodies: compiled.bodies.clone(),
        parts,
        focus: Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32),
        home: None,
    };
    Ok((compiled, scene))
}

/// Compile the starter robot and start the sim thread.
pub fn start(starter: &str, volts: &[(String, f64)]) -> Result<(SandboxScene, SandboxSim), String> {
    let (compiled, scene) = build_robot(starter)?;
    let sim = Sandbox::spawn(&compiled, volts).map_err(|e| e.to_string())?;
    Ok((scene, SandboxSim(Mutex::new(sim))))
}

/// The mission the live view shows: the robot in its world, the setup to start programs in it,
/// and the seed (always the first grading seed, so what you watch is what grading sees).
#[derive(Resource)]
pub struct CodeRobot {
    /// Robot, world and scenario of this seed.
    pub setup: axl_sim::mission::MissionSetup,
    /// The mission (or the sandbox).
    pub def: MissionDef,
    /// Seed of the world, mat layout and sensor noise.
    pub seed: u64,
}

/// A world on the stage with a robot standing in it.
pub struct LiveParts {
    /// The robot's bodies.
    pub scene: SandboxScene,
    /// The mat, walls, objects and zones.
    pub world: WorldScene,
    /// The sim thread (robot standing still).
    pub sim: SandboxSim,
    /// What is needed to start programs.
    pub robot: CodeRobot,
}

fn object_kind(s: &axl_world::objects::ObjShape) -> ObjectKind {
    match *s {
        axl_world::objects::ObjShape::Cube(a) => ObjectKind::Cube(a),
        axl_world::objects::ObjShape::Cylinder { diameter, height } => {
            ObjectKind::Cylinder(diameter, height)
        }
        axl_world::objects::ObjShape::Sphere(d) => ObjectKind::Sphere(d),
    }
}

/// What the renderer draws for a mission setup.
pub fn world_scene(setup: &axl_sim::mission::MissionSetup, def: &MissionDef) -> WorldScene {
    let mat =
        axl_world::build::display_mat(&setup.world, setup.variant.as_deref()).map(|m| MatImage {
            min: m.min,
            max: m.max,
            width: m.width,
            height: m.height,
            pixels: m.pixels,
        });
    let statics = setup
        .shapes
        .iter()
        .map(|s| StaticDraw {
            kind: match s.kind {
                axl_world::build::ShapeKind::Box(h) => StaticKind::Box(h),
                axl_world::build::ShapeKind::Cylinder(r, h) => StaticKind::Cylinder(r, h),
            },
            pos: s.pos,
            quat: s.quat,
            color: s.color,
            material: StaticMaterial::parse(&s.material),
        })
        .collect();
    let ids = setup.object_body_ids().unwrap_or_default();
    let objects = setup
        .objects
        .iter()
        .zip(ids)
        .filter_map(|(o, id)| {
            Some(ObjectDraw {
                body: id?,
                kind: object_kind(&o.shape),
                color: o.color,
            })
        })
        .collect();
    let named = axl_missions::results::zone_names(def);
    let zones = setup
        .eval
        .zones
        .iter()
        .filter(|z| named.contains(&z.name))
        .map(|z| ZoneDraw {
            name: z.name.clone(),
            shape: match z.shape {
                axl_world::ZoneShape::Rect { min, max } => ZoneShape::Rect { min, max },
                axl_world::ZoneShape::Circle { center, radius } => {
                    ZoneShape::Circle { center, radius }
                }
            },
        })
        .collect();
    WorldScene {
        mat,
        statics,
        objects,
        zones,
    }
}

/// Put the robot of `def` into its world for `seed` and start the sim thread with the robot
/// standing still. `design` is the learner's `.axlbot` for missions that need one.
pub fn open_live(
    def: &MissionDef,
    seed: u64,
    design: Option<&std::path::Path>,
) -> Result<LiveParts, String> {
    let cat = Catalog::builtin();
    let asm = robot_assembly(def, &cat, design).map_err(|e| e.to_string())?;
    open_live_with(def, seed, &asm)
}

/// Like [`open_live`], with the robot given (the builder's design in the sandbox).
pub fn open_live_with(
    def: &MissionDef,
    seed: u64,
    asm: &axl_build::Assembly,
) -> Result<LiveParts, String> {
    let cat = Catalog::builtin();
    let setup = axl_sim::mission::setup(def, seed, asm, &cat).map_err(|e| e.to_string())?;
    let parts: BTreeMap<u32, (String, String)> = asm
        .parts
        .iter()
        .map(|p| (p.uid, (p.part.0.clone(), p.color.0.clone())))
        .collect();
    let o = setup
        .compiled
        .model
        .bodies
        .first()
        .map(|b| b.origin)
        .unwrap_or([0.0; 3]);
    let world = world_scene(&setup, def);
    let home = world.extent().map(|(c, r)| CameraHome {
        focus: Vec3::new(c[0] as f32, c[1] as f32, 0.0),
        yaw_deg: -50.0,
        pitch_deg: 40.0,
        distance: ((r as f32) * 2.4).max(0.8),
    });
    let scene = SandboxScene {
        bodies: setup.compiled.bodies.clone(),
        parts,
        focus: Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32),
        home,
    };
    let sim = setup
        .spawn_live(def, None, true)
        .map_err(|e| e.to_string())?;
    Ok(LiveParts {
        scene,
        world,
        sim: SandboxSim(Mutex::new(sim)),
        robot: CodeRobot {
            setup,
            def: def.clone(),
            seed,
        },
    })
}

/// Start a built program in the mission's world.
pub fn run_program(robot: &CodeRobot, wasm: Vec<u8>) -> Result<SandboxSim, String> {
    robot
        .setup
        .spawn_live(&robot.def, Some(wasm), true)
        .map(|s| SandboxSim(Mutex::new(s)))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volts_parse() {
        assert_eq!(
            parse_volts("left=4, right=-2.5").unwrap(),
            vec![("left".into(), 4.0), ("right".into(), -2.5)]
        );
        assert!(parse_volts("left").is_err());
        assert!(parse_volts("left=x").is_err());
        assert!(parse_volts("").unwrap().is_empty());
    }
}
