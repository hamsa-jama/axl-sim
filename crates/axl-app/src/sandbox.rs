//! The sandbox: a starter robot driven by fixed motor voltages on the sim thread, drawn through
//! the body map. This is the M5 stand-in for the Simulate screen.

use std::collections::BTreeMap;
use std::sync::Mutex;

use axl_build::starter::build_starter;
use axl_compile::{CompileOptions, PhysicsParams, compile};
use axl_parts::catalog::Catalog;
use axl_render::bevy::prelude::*;
use axl_render::sim_view::SandboxScene;
use axl_render::sync::{BodyPose, RenderSnapshot, SnapshotBuffer};
use axl_sim::Sandbox;

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

/// Compile the starter robot and start the sim thread.
pub fn start(starter: &str, volts: &[(String, f64)]) -> Result<(SandboxScene, SandboxSim), String> {
    let cat = Catalog::builtin();
    let asm = build_starter(&cat, starter).map_err(|e| e.to_string())?;
    let compiled = compile(
        &cat,
        &asm,
        &PhysicsParams::builtin(),
        &CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let sim = Sandbox::spawn(&compiled, volts).map_err(|e| e.to_string())?;
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
    };
    Ok((scene, SandboxSim(Mutex::new(sim))))
}

/// Move the newest sim snapshot into the render buffer, once per new snapshot.
pub fn pump(
    sim: Res<SandboxSim>,
    time: Res<Time<Real>>,
    mut buf: ResMut<SnapshotBuffer>,
    mut last: Local<f64>,
) {
    let Ok(mut sb) = sim.0.lock() else { return };
    let s = sb.latest();
    if s.bodies.is_empty() || s.time == *last {
        return;
    }
    *last = s.time;
    buf.push(
        RenderSnapshot {
            time: s.time,
            bodies: s
                .bodies
                .iter()
                .map(|p| BodyPose {
                    pos: p.pos,
                    quat: p.quat,
                })
                .collect(),
        },
        time.elapsed_secs_f64(),
    );
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
