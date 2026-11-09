//! The sandbox view: a compiled robot drawn from the body map and moved by the snapshots.
//!
//! Each MuJoCo body becomes one [`SimBody`] entity; the parts it holds (from the compiler's
//! `BodyMap`) hang below it at their fixed offsets. A motor's housing and rotor are two
//! sub-bodies of one part, so they are spawned from separate mesh slots.

use std::collections::BTreeMap;

use axl_build::Orient;
use axl_compile::BodyMap;
use bevy::prelude::*;

use crate::builder_view::mat3;
use crate::camera_rig::CameraHome;
use crate::scene::ShapeBuilder;
use crate::sync::SimBody;

/// What the sandbox draws: the body map and which catalog part and colour each uid is.
#[derive(Resource, Clone, Debug)]
pub struct SandboxScene {
    /// Bodies and their parts.
    pub bodies: BodyMap,
    /// Part uid to (catalog id, colour id).
    pub parts: BTreeMap<u32, (String, String)>,
    /// Where the camera looks at the start (AXL frame, metres).
    pub focus: Vec3,
}

/// Spawn one entity per simulated body under `root`. Returns the camera home.
pub fn spawn_sandbox(
    commands: &mut Commands,
    root: Entity,
    scene: &SandboxScene,
    builder: &mut ShapeBuilder,
) -> CameraHome {
    for body in &scene.bodies.bodies {
        let e = commands
            .spawn((
                SimBody(body.id),
                Name::new(body.name.clone()),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        commands.entity(root).add_child(e);
        for p in &body.parts {
            let Some((part, color)) = scene.parts.get(&p.uid) else {
                continue;
            };
            let Some(orient) = Orient::from_index(p.orient) else {
                continue;
            };
            let tf = Transform::from_translation(Vec3::new(
                p.pos[0] as f32,
                p.pos[1] as f32,
                p.pos[2] as f32,
            ))
            .with_rotation(Quat::from_mat3(&mat3(orient)));
            builder.spawn_part_slots(commands, e, part, color, p.uid, tf, Some(p.sub as usize));
        }
    }
    tracing::info!(
        "sandbox: {} bodies, {} parts",
        scene.bodies.bodies.len(),
        scene.parts.len()
    );
    CameraHome {
        focus: scene.focus,
        yaw_deg: -50.0,
        pitch_deg: 30.0,
        distance: 0.45,
    }
}
