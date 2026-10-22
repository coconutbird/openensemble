//! Scenario-authored design objects: spheres and line path geometry.
//!
//! The engine keeps them in the design object manager at `BWorld+624`
//! (`BDesignObjectManager_load` @ `0x140200530`). Triggers read the spheres
//! with `CanGetDesignSpheres` and the lines with the design-line effects.

use super::World;
use crate::sync::SyncChecksum;
use glam::Vec3;
use std::collections::BTreeMap;

/// Retail identity for one scenario design line.
pub type DesignLineId = i32;

/// One scenario design sphere in canonical world axes.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignSphere {
    /// The `Data/Type` text that triggers select the sphere by.
    pub type_name: String,
    /// The sphere center.
    pub center: Vec3,
    /// The sphere radius.
    pub radius: f32,
}

#[derive(Debug, Default)]
pub(super) struct DesignObjectState {
    spheres: Vec<DesignSphere>,
    line_points_by_id: BTreeMap<DesignLineId, Vec<Vec3>>,
}

impl DesignObjectState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.spheres.len()).unwrap_or(u32::MAX));
        for sphere in &self.spheres {
            checksum.hash_string(&sphere.type_name);
            checksum.hash_vec3(sphere.center.x, sphere.center.y, sphere.center.z);
            checksum.hash_f32(sphere.radius);
        }
        checksum.hash_u32(u32::try_from(self.line_points_by_id.len()).unwrap_or(u32::MAX));
        for (line_id, points) in &self.line_points_by_id {
            checksum.hash_i32(*line_id);
            checksum.hash_u32(u32::try_from(points.len()).unwrap_or(u32::MAX));
            for point in points {
                checksum.hash_vec3(point.x, point.y, point.z);
            }
        }
    }
}

impl World {
    /// Replace the scenario design objects.
    ///
    /// The spheres keep their order. The world keeps the last line for each
    /// duplicate line ID.
    pub(crate) fn configure_design_objects(
        &mut self,
        spheres: Vec<DesignSphere>,
        lines: impl IntoIterator<Item = (DesignLineId, Vec<Vec3>)>,
    ) {
        self.design_objects = DesignObjectState {
            spheres,
            line_points_by_id: lines.into_iter().collect(),
        };
    }

    /// Return the scenario design spheres in the engine array order.
    #[must_use]
    pub fn design_spheres(&self) -> &[DesignSphere] {
        &self.design_objects.spheres
    }

    /// Return the ordered canonical-world points for a scenario design line.
    #[must_use]
    pub fn design_line_points(&self, line_id: DesignLineId) -> Option<&[Vec3]> {
        self.design_objects
            .line_points_by_id
            .get(&line_id)
            .map(Vec::as_slice)
    }

    /// Return the number of scenario design lines loaded into the simulation.
    #[must_use]
    pub fn design_line_count(&self) -> usize {
        self.design_objects.line_points_by_id.len()
    }
}
