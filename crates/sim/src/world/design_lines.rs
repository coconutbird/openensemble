//! Scenario-authored design-line path geometry.

use super::World;
use crate::sync::SyncChecksum;
use glam::Vec3;
use std::collections::BTreeMap;

/// Retail identity for one scenario design line.
pub type DesignLineId = i32;

#[derive(Debug, Default)]
pub(super) struct DesignLineState {
    points_by_id: BTreeMap<DesignLineId, Vec<Vec3>>,
}

impl DesignLineState {
    fn replace(&mut self, lines: impl IntoIterator<Item = (DesignLineId, Vec<Vec3>)>) {
        self.points_by_id.clear();
        self.points_by_id.extend(lines);
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.points_by_id.len()).unwrap_or(u32::MAX));
        for (line_id, points) in &self.points_by_id {
            checksum.hash_i32(*line_id);
            checksum.hash_u32(u32::try_from(points.len()).unwrap_or(u32::MAX));
            for point in points {
                checksum.hash_vec3(point.x, point.y, point.z);
            }
        }
    }
}

impl World {
    pub(crate) fn configure_design_lines(
        &mut self,
        lines: impl IntoIterator<Item = (DesignLineId, Vec<Vec3>)>,
    ) {
        self.design_lines.replace(lines);
    }

    /// Return the ordered canonical-world points for a scenario design line.
    #[must_use]
    pub fn design_line_points(&self, line_id: DesignLineId) -> Option<&[Vec3]> {
        self.design_lines
            .points_by_id
            .get(&line_id)
            .map(Vec::as_slice)
    }

    /// Return the number of scenario design lines loaded into the simulation.
    #[must_use]
    pub fn design_line_count(&self) -> usize {
        self.design_lines.points_by_id.len()
    }
}
