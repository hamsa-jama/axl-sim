//! Changing what the stage shows while the app runs: the screens of AXL show different things on
//! the same floor and camera.
//!
//! * [`StageKind::Backdrop`]: the look-dev scene behind the menu screens.
//! * [`StageKind::Builder`]: the robot being built.
//! * [`StageKind::Sim`]: a simulation: the robot ([`SandboxScene`]) and, in a mission, its world
//!   ([`WorldScene`]). Replace either resource and the sim stage is rebuilt.

use bevy::prelude::*;

use crate::camera_rig::{ActiveQuality, FocusTarget};
use crate::diagnostics::DisableSet;
use crate::frames::AxlRoot;
use crate::parts::PartLibrary;
use crate::scene::{PartMaterials, ShapeBuilder};
use crate::sim_view::{SandboxScene, spawn_sandbox};
use crate::world_view::{WorldScene, spawn_world};

/// What the stage shows now.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StageKind {
    /// The backdrop scene behind Home, Missions and Settings.
    #[default]
    Backdrop,
    /// The builder.
    Builder,
    /// A simulation.
    Sim,
}

/// Parent of everything in the simulation view; shown only for [`StageKind::Sim`].
#[derive(Component)]
pub struct SimStageRoot;

/// Parent of the backdrop scene; shown only for [`StageKind::Backdrop`].
#[derive(Component)]
pub struct BackdropRoot;

/// Run condition: the stage shows the builder.
pub fn in_builder(kind: Res<StageKind>) -> bool {
    *kind == StageKind::Builder
}

/// Run condition: the stage shows a simulation.
pub fn in_sim(kind: Res<StageKind>) -> bool {
    *kind == StageKind::Sim
}

/// Show the root of the current stage and hide the others.
pub fn apply_stage_visibility(
    kind: Res<StageKind>,
    mut q: Query<(&mut Visibility, Has<SimStageRoot>, Has<BackdropRoot>)>,
) {
    for (mut v, sim, back) in &mut q {
        let show = (sim && *kind == StageKind::Sim) || (back && *kind == StageKind::Backdrop);
        let want = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

/// Rebuild the simulation view when its scene changes, and point the camera at it.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_sim_stage(
    mut commands: Commands,
    scene: Option<Res<SandboxScene>>,
    world: Option<Res<WorldScene>>,
    old: Query<Entity, With<SimStageRoot>>,
    roots: Query<Entity, With<AxlRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut parts: ResMut<PartMaterials>,
    mut lib: ResMut<PartLibrary>,
    quality: Res<ActiveQuality>,
    disable: Res<DisableSet>,
    mut had: Local<bool>,
) {
    let present = scene.is_some();
    let changed = scene.as_ref().is_some_and(|s| s.is_changed())
        || world.as_ref().is_some_and(|w| w.is_changed());
    if !changed && present == *had {
        return;
    }
    *had = present;
    for e in &old {
        commands.entity(e).despawn();
    }
    let (Some(scene), Ok(root)) = (scene, roots.single()) else {
        return;
    };
    let stage = commands
        .spawn((
            SimStageRoot,
            Name::new("sim_stage"),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(root).add_child(stage);
    if let Some(w) = &world {
        spawn_world(
            &mut commands,
            stage,
            w,
            &mut meshes,
            &mut std_mats,
            &mut images,
        );
    }
    let mut builder = ShapeBuilder {
        meshes: &mut meshes,
        materials: &mut std_mats,
        parts: &mut parts,
        lib: &mut lib,
        micro_on: quality.0.settings().micro_surface && !disable.micro_normal,
    };
    let (home, focus) = spawn_sandbox(&mut commands, stage, &scene, &mut builder);
    commands.insert_resource(scene.home.unwrap_or(home));
    commands.insert_resource(FocusTarget(focus));
}

/// Registers the stage kind and the systems that swap content.
pub struct StageSwapPlugin(pub StageKind);

impl Plugin for StageSwapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0).add_systems(
            Update,
            (
                rebuild_sim_stage.before(crate::sync::sync_bodies),
                apply_stage_visibility,
            )
                .chain(),
        );
    }
}
