//! The one place where AXL's Z-up frame meets Bevy's Y-up frame.
//!
//! Every AXL entity lives under a single root entity whose rotation is
//! [`root_rotation`] (-90 degrees about X). That one rotation maps AXL
//! `(x, y, z)` to Bevy `(x, z, -y)`. No other code converts frames: meshes,
//! snapshots and the grid shader all work in AXL coordinates.

use bevy::prelude::*;

/// Marks the root entity that holds every AXL entity.
#[derive(Component, Debug, Default)]
pub struct AxlRoot;

/// The root's rotation: -90 degrees about X.
pub fn root_rotation() -> Quat {
    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

/// Where an AXL-frame point ends up in Bevy world space (with the root at the origin).
///
/// Only used for the camera, which sits outside the root, and for tests.
pub fn axl_to_bevy(p: Vec3) -> Vec3 {
    root_rotation() * p
}

/// The inverse of [`axl_to_bevy`].
pub fn bevy_to_axl(p: Vec3) -> Vec3 {
    root_rotation().inverse() * p
}

/// Spawn the AXL root entity at the world origin.
pub fn spawn_root(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            AxlRoot,
            Name::new("axl_root"),
            Transform::from_rotation(root_rotation()),
            Visibility::default(),
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn maps_xyz_to_x_z_minus_y() {
        assert!(close(axl_to_bevy(Vec3::X), Vec3::X));
        assert!(close(axl_to_bevy(Vec3::Y), Vec3::new(0.0, 0.0, -1.0)));
        assert!(close(axl_to_bevy(Vec3::Z), Vec3::Y));
        assert!(close(
            axl_to_bevy(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(1.0, 3.0, -2.0)
        ));
    }

    #[test]
    fn round_trip() {
        let p = Vec3::new(0.3, -0.7, 0.11);
        assert!(close(bevy_to_axl(axl_to_bevy(p)), p));
    }
}
