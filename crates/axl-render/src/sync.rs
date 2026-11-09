//! Body sync: moving entities to the poses in the two newest snapshots.
//!
//! The sim thread (milestone M5/M6) publishes a snapshot of every body pose up to
//! 250 times a second. The renderer keeps the two newest and, at display rate,
//! blends between them. Poses are in the AXL frame (Z-up) because the entities
//! sit under the AXL root; no frame conversion happens here.

use bevy::prelude::*;

/// A body pose as the physics engine reports it: position and a `[w, x, y, z]` quaternion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPose {
    /// Position in metres, AXL frame.
    pub pos: [f64; 3],
    /// Orientation as `[w, x, y, z]` (MuJoCo order).
    pub quat: [f64; 4],
}

impl BodyPose {
    /// The identity pose at the origin.
    pub const IDENTITY: BodyPose = BodyPose {
        pos: [0.0; 3],
        quat: [1.0, 0.0, 0.0, 0.0],
    };

    fn to_render(self) -> (Vec3, Quat) {
        let q = Quat::from_xyzw(
            self.quat[1] as f32,
            self.quat[2] as f32,
            self.quat[3] as f32,
            self.quat[0] as f32,
        );
        (
            Vec3::new(self.pos[0] as f32, self.pos[1] as f32, self.pos[2] as f32),
            if q.length_squared() > 0.0 {
                q.normalize()
            } else {
                Quat::IDENTITY
            },
        )
    }
}

/// What the renderer needs from one published snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    /// Simulation time in seconds.
    pub time: f64,
    /// Pose of every body, indexed by body id.
    pub bodies: Vec<BodyPose>,
}

/// Blend two poses. `alpha` is 0 for `a`, 1 for `b`.
pub fn interpolate_pose(a: &BodyPose, b: &BodyPose, alpha: f32) -> (Vec3, Quat) {
    let (pa, qa) = a.to_render();
    let (pb, qb) = b.to_render();
    let alpha = alpha.clamp(0.0, 1.0);
    (pa.lerp(pb, alpha), qa.slerp(qb, alpha))
}

/// The two newest snapshots, and when the newest arrived.
#[derive(Resource, Default, Debug)]
pub struct SnapshotBuffer {
    previous: Option<RenderSnapshot>,
    latest: Option<RenderSnapshot>,
    latest_arrival_s: f64,
}

impl SnapshotBuffer {
    /// Add a snapshot that arrived at wall-clock time `now_s`.
    pub fn push(&mut self, snapshot: RenderSnapshot, now_s: f64) {
        self.previous = self.latest.take();
        self.latest = Some(snapshot);
        self.latest_arrival_s = now_s;
    }

    /// The newest snapshot, if any.
    pub fn latest(&self) -> Option<&RenderSnapshot> {
        self.latest.as_ref()
    }

    /// Forget everything (after a reset or rewind).
    pub fn clear(&mut self) {
        self.previous = None;
        self.latest = None;
    }

    /// Pose of body `id` at wall-clock time `now_s`, blended between the two newest snapshots.
    ///
    /// Blend progress is the time since the newest snapshot arrived divided by the
    /// simulated gap between the two snapshots (a gap of 0 means no blending).
    pub fn pose(&self, id: usize, now_s: f64, speed: f64) -> Option<(Vec3, Quat)> {
        let latest = self.latest.as_ref()?;
        let newest = latest.bodies.get(id)?;
        let Some(prev) = self.previous.as_ref().and_then(|p| p.bodies.get(id)) else {
            return Some(newest.to_render());
        };
        let gap = latest.time - self.previous.as_ref()?.time;
        if gap <= 0.0 {
            return Some(newest.to_render());
        }
        let alpha = ((now_s - self.latest_arrival_s) * speed / gap).clamp(0.0, 1.0) as f32;
        Some(interpolate_pose(prev, newest, alpha))
    }
}

/// Attach to an entity to make it follow body `0` of the snapshots.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimBody(pub usize);

/// Playback speed used to scale blend progress (1.0 is real time).
#[derive(Resource, Clone, Copy, Debug)]
pub struct PlaybackSpeed(pub f64);

impl Default for PlaybackSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Moves every [`SimBody`] entity to its interpolated pose.
pub fn sync_bodies(
    buffer: Res<SnapshotBuffer>,
    speed: Res<PlaybackSpeed>,
    time: Res<Time<Real>>,
    mut bodies: Query<(&SimBody, &mut Transform)>,
) {
    let now = time.elapsed_secs_f64();
    for (body, mut tf) in &mut bodies {
        if let Some((pos, rot)) = buffer.pose(body.0, now, speed.0) {
            tf.translation = pos;
            tf.rotation = rot;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(t: f64, x: f64) -> RenderSnapshot {
        RenderSnapshot {
            time: t,
            bodies: vec![BodyPose {
                pos: [x, 0.0, 0.0],
                quat: [1.0, 0.0, 0.0, 0.0],
            }],
        }
    }

    #[test]
    fn no_snapshot_means_no_pose() {
        assert!(SnapshotBuffer::default().pose(0, 0.0, 1.0).is_none());
    }

    #[test]
    fn single_snapshot_is_used_as_is() {
        let mut b = SnapshotBuffer::default();
        b.push(snap(0.0, 1.0), 10.0);
        let (p, _) = b.pose(0, 10.5, 1.0).unwrap();
        assert!((p.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn blends_between_two_newest() {
        let mut b = SnapshotBuffer::default();
        b.push(snap(0.000, 0.0), 0.0);
        b.push(snap(0.004, 4.0), 1.0);
        // Half of the 4 ms gap has passed since the newest arrived.
        let (p, _) = b.pose(0, 1.002, 1.0).unwrap();
        assert!((p.x - 2.0).abs() < 1e-5, "{}", p.x);
        // After the full gap we sit exactly on the newest pose and never overshoot.
        let (p, _) = b.pose(0, 1.5, 1.0).unwrap();
        assert!((p.x - 4.0).abs() < 1e-6);
        // Before the newest arrived: the older pose.
        let (p, _) = b.pose(0, 0.9, 1.0).unwrap();
        assert!(p.x.abs() < 1e-6);
    }

    #[test]
    fn speed_scales_progress() {
        let mut b = SnapshotBuffer::default();
        b.push(snap(0.0, 0.0), 0.0);
        b.push(snap(0.004, 4.0), 1.0);
        // At 4x speed, 1 ms of wall time covers 4 ms of sim time: the full gap.
        let (p, _) = b.pose(0, 1.001, 4.0).unwrap();
        assert!((p.x - 4.0).abs() < 1e-5);
    }

    #[test]
    fn rotation_is_slerped_and_quaternion_order_is_wxyz() {
        let a = BodyPose::IDENTITY;
        let half = std::f64::consts::FRAC_PI_4; // half of a 90 degree turn about Z
        let b = BodyPose {
            pos: [0.0; 3],
            quat: [half.cos(), 0.0, 0.0, half.sin()],
        };
        let (_, q) = interpolate_pose(&a, &b, 0.5);
        let (axis, angle) = q.to_axis_angle();
        assert!((angle - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
        assert!((axis - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn unknown_body_is_none_and_clear_works() {
        let mut b = SnapshotBuffer::default();
        b.push(snap(0.0, 1.0), 0.0);
        assert!(b.pose(5, 0.0, 1.0).is_none());
        b.clear();
        assert!(b.latest().is_none());
    }

    #[test]
    fn zero_quaternion_does_not_poison_transform() {
        let bad = BodyPose {
            pos: [0.0; 3],
            quat: [0.0; 4],
        };
        let (_, q) = interpolate_pose(&bad, &bad, 0.5);
        assert!(q.is_finite() && (q.length() - 1.0).abs() < 1e-5);
    }
}
