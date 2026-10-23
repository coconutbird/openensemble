//! Ground contact against the retail simulation terrain.
//!
//! Retail ties a ground entity to the terrain in `BEntity_tieToGround` @ `0x140202EB0`. A
//! Havok-driven vehicle skips that tie. Its rigid body collides with a
//! `hkpSampledHeightFieldShape` over the same height grid
//! (`BTerrainPhysics_createHeightFieldBody` @ `0x14041B500`). This module holds the terrain
//! plane that a [`PhysicsBody`] collides with and the retail ground-orientation math.

use super::{PhysicsBody, hash_vec3, planar};
use crate::entities::BaseEntity;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Half width of the ground footprint along the right axis (`BEntity_orientWithGround`).
const ORIENT_HALF_WIDTH: f32 = 1.25;
/// Half length of the ground footprint along the forward axis (`BEntity_orientWithGround`).
const ORIENT_HALF_LENGTH: f32 = 2.25;
/// Weight of the new frame in `BEntity_filterOrientation` @ `0x1402035F0`.
const ORIENT_FILTER_WEIGHT: f32 = 0.1;
/// Smallest rebound speed that lifts a landing body off the ground again.
const GROUND_SNAP_SPEED: f32 = 0.5;
/// Height tolerance for a body that keeps contact with the ground.
const GROUND_CONTACT_SLOP: f32 = 0.001;
/// Smallest normal Y component that the terrain plane accepts.
const MIN_NORMAL_Y: f32 = 0.01;

/// Terrain height and orientation under an entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroundSample {
    /// Mean terrain height of the four footprint samples.
    pub height: f32,
    /// Unit terrain normal with a positive Y component.
    pub normal: Vec3,
}

/// Sample the terrain under an entity like `BEntity_orientWithGround` @ `0x1402032A0`.
///
/// Retail samples the clamped terrain height at the four corners `±1.25 right ± 2.25 forward`.
/// The height is the mean of the four samples. The normal is the cross product of the diagonal
/// with the higher mean height and half of the other diagonal, turned to point up.
pub(crate) fn sample_ground(
    position: Vec3,
    heading: Vec3,
    mut height_at: impl FnMut(Vec3) -> Option<f32>,
) -> Option<GroundSample> {
    let forward = planar(heading).try_normalize().unwrap_or(Vec3::Z);
    let right = Vec3::Y.cross(forward);
    let mut corner = |side: f32, along: f32| {
        let point =
            position + right * (side * ORIENT_HALF_WIDTH) + forward * (along * ORIENT_HALF_LENGTH);
        height_at(point).map(|height| Vec3::new(point.x, height, point.z))
    };
    let front_right = corner(1.0, 1.0)?;
    let back_right = corner(1.0, -1.0)?;
    let front_left = corner(-1.0, 1.0)?;
    let back_left = corner(-1.0, -1.0)?;
    let first_mean = f32::midpoint(front_right.y, back_left.y);
    let second_mean = f32::midpoint(back_right.y, front_left.y);
    let (diagonal, other) = if first_mean >= second_mean {
        (front_right - back_left, (front_left - back_right) * 0.5)
    } else {
        (back_right - front_left, (back_left - front_right) * 0.5)
    };
    let mut normal = diagonal.cross(other);
    if normal.y < 0.0 {
        normal = -normal;
    }
    let normal = normal.try_normalize().unwrap_or(Vec3::Y);
    let height = (front_right.y + back_right.y + front_left.y + back_left.y) * 0.25;
    height
        .is_finite()
        .then_some(GroundSample { height, normal })
}

/// Return the right and forward axes of a frame with this up axis and planar heading.
///
/// This is the projection in `BEntity_orientWithGround`: `right = up × heading`,
/// `forward = right × up`.
pub(crate) fn ground_frame(up: Vec3, heading: Vec3) -> (Vec3, Vec3) {
    let heading = planar(heading).try_normalize().unwrap_or(Vec3::Z);
    let right = up.cross(heading).try_normalize().unwrap_or(Vec3::X);
    let forward = right.cross(up).try_normalize().unwrap_or(heading);
    (right, forward)
}

/// Blend the previous up axis toward a new terrain normal like `BEntity_filterOrientation`.
///
/// Retail blends the right and forward axes with weight 0.1 for the new frame. It then sets
/// `up = forward × right`. Retail skips the filter when the previous up axis is exactly world
/// up (`BEntity_tieToGround` @ `0x140202EB0`).
pub(crate) fn filter_up(previous_up: Vec3, normal: Vec3, heading: Vec3) -> Vec3 {
    if previous_up.y.to_bits() == 1.0_f32.to_bits() {
        return normal;
    }
    let (previous_right, previous_forward) = ground_frame(previous_up, heading);
    let (right, forward) = ground_frame(normal, heading);
    let right = right * ORIENT_FILTER_WEIGHT + previous_right * (1.0 - ORIENT_FILTER_WEIGHT);
    let forward = forward * ORIENT_FILTER_WEIGHT + previous_forward * (1.0 - ORIENT_FILTER_WEIGHT);
    forward.cross(right).try_normalize().unwrap_or(normal)
}

/// Terrain plane that a physics body collides with during one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GroundPlane {
    origin: Vec3,
    normal: Vec3,
}

impl GroundPlane {
    /// Return a horizontal plane at a fixed height.
    pub(super) fn flat(height: f32) -> Self {
        Self {
            origin: Vec3::new(0.0, height, 0.0),
            normal: Vec3::Y,
        }
    }

    fn from_sample(position: Vec3, sample: GroundSample) -> Self {
        let normal = if sample.normal.is_finite() && sample.normal.y >= MIN_NORMAL_Y {
            sample.normal
        } else {
            Vec3::Y
        };
        Self {
            origin: Vec3::new(position.x, sample.height, position.z),
            normal,
        }
    }

    /// Return the plane height under a point.
    pub(super) fn height_at(self, position: Vec3) -> f32 {
        let slope = self.normal.x * (position.x - self.origin.x)
            + self.normal.z * (position.z - self.origin.z);
        self.origin.y - slope / self.normal.y
    }

    /// Return the vertical speed of a point that slides on the plane with this velocity.
    pub(super) fn surface_rate(self, velocity: Vec3) -> f32 {
        -(self.normal.x * velocity.x + self.normal.z * velocity.z) / self.normal.y
    }

    pub(super) fn hash_state(self, checksum: &mut SyncChecksum) {
        hash_vec3(checksum, self.origin);
        hash_vec3(checksum, self.normal);
    }
}

impl PhysicsBody {
    /// Collide with the terrain under the body during the next step.
    pub(crate) fn set_ground(&mut self, position: Vec3, sample: GroundSample) {
        self.ground = GroundPlane::from_sample(position, sample);
    }

    /// Return the normal of the terrain plane that the body collides with.
    #[must_use]
    pub const fn ground_normal(&self) -> Vec3 {
        self.ground.normal
    }

    /// Return the height of the terrain plane under a point.
    #[must_use]
    pub fn ground_height_at(&self, position: Vec3) -> f32 {
        self.ground.height_at(position)
    }

    /// Return whether a velocity moves the body up and away from the terrain plane.
    pub(super) fn leaves_ground(&self, velocity: Vec3) -> bool {
        velocity.y > self.ground.surface_rate(velocity)
    }

    /// Make a grounded body slide on the terrain plane.
    pub(super) fn follow_ground(&self, entity: &mut BaseEntity) {
        if self.grounded {
            entity.velocity.y = self.ground.surface_rate(entity.velocity);
        }
    }

    /// Keep the body on or above the terrain plane after the position step.
    ///
    /// A grounded body keeps contact while the terrain falls away by no more than the planar
    /// distance of the step. A steeper drop makes it airborne, and gravity then acts on it.
    pub(super) fn resolve_ground(&mut self, entity: &mut BaseEntity, planar_step: f32) {
        let height = self.ground.height_at(entity.position);
        let rate = self.ground.surface_rate(entity.velocity);
        let gap = entity.position.y - height;
        if self.grounded {
            if gap > planar_step + GROUND_CONTACT_SLOP {
                self.grounded = false;
                return;
            }
            self.land(entity, height, rate);
            return;
        }
        if gap > 0.0 || entity.velocity.y > rate {
            return;
        }
        entity.position.y = height;
        let approach = rate - entity.velocity.y;
        self.ground_impact_speed_this_step = approach;
        let rebound = approach * self.material.restitution;
        if rebound > GROUND_SNAP_SPEED {
            entity.velocity.y = rate + rebound;
        } else {
            self.land(entity, height, rate);
        }
    }

    fn land(&mut self, entity: &mut BaseEntity, height: f32, rate: f32) {
        entity.position.y = height;
        entity.velocity.y = rate;
        entity.up = self.ground.normal;
        self.grounded = true;
    }
}

#[cfg(test)]
mod tests;
