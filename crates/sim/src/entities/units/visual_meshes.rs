//! Authoritative visual-mesh state shared by damage actions and presentation.

use super::Unit;
use crate::EntityId;
use crate::gameplay::ThrownDamagePart;
use crate::physics::PhysicsBody;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// Per-unit mesh visibility corresponding to retail's Granny render mask.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitVisualMeshMask {
    only: Vec<String>,
    hidden: Vec<String>,
    hidden_components: Vec<String>,
    section_overrides: Vec<Option<bool>>,
    revision: u32,
}

impl UnitVisualMeshMask {
    /// Meshes exclusively rendered for this unit; empty means all meshes.
    #[must_use]
    pub fn only(&self) -> &[String] {
        &self.only
    }

    /// Meshes removed from the current render mask.
    #[must_use]
    pub fn hidden(&self) -> &[String] {
        &self.hidden
    }

    /// Recursive visual-component names hidden by authoritative simulation.
    #[must_use]
    pub fn hidden_components(&self) -> &[String] {
        &self.hidden_components
    }

    /// Return whether one recursive visual component should be projected.
    #[must_use]
    pub fn is_component_visible(&self, name: &str) -> bool {
        !self
            .hidden_components
            .iter()
            .any(|hidden| hidden.eq_ignore_ascii_case(name.trim()))
    }

    /// Per-section visibility overrides in retail Granny mask-bit order.
    #[must_use]
    pub fn section_overrides(&self) -> &[Option<bool>] {
        &self.section_overrides
    }

    /// Monotonic state revision used to refresh presentation resources.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    fn show_only(&mut self, names: &[String]) {
        let only = normalized(names.iter().map(String::as_str));
        if self.only != only || !self.hidden.is_empty() {
            self.only = only;
            self.hidden.clear();
            self.bump_revision();
        }
    }

    fn hide(&mut self, names: &[String]) {
        let mut hidden = self.hidden.clone();
        hidden.extend(names.iter().map(|name| name.to_ascii_lowercase()));
        hidden.sort_unstable();
        hidden.dedup();
        if self.hidden != hidden {
            self.hidden = hidden;
            self.bump_revision();
        }
    }

    fn set_section_visibility(&mut self, index: usize, visible: bool) {
        if self.section_overrides.len() <= index {
            self.section_overrides.resize(index + 1, None);
        }
        if self.section_overrides[index] != Some(visible) {
            self.section_overrides[index] = Some(visible);
            self.bump_revision();
        }
    }

    fn set_component_visibility(&mut self, name: &str, visible: bool) {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return;
        }
        let previous = self.hidden_components.clone();
        if visible {
            self.hidden_components.retain(|hidden| hidden != &name);
        } else if !self.hidden_components.contains(&name) {
            self.hidden_components.push(name);
            self.hidden_components.sort_unstable();
        }
        if previous != self.hidden_components {
            self.bump_revision();
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        if self.revision == 0 {
            self.revision = 1;
        }
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_names(checksum, &self.only);
        hash_names(checksum, &self.hidden);
        hash_names(checksum, &self.hidden_components);
        checksum.hash_u32(u32::try_from(self.section_overrides.len()).unwrap_or(u32::MAX));
        for override_value in &self.section_overrides {
            checksum.hash_u32(override_value.map_or(u32::MAX, u32::from));
        }
        checksum.hash_u32(self.revision);
    }
}

impl Unit {
    /// Return the simulation-owned Granny mesh visibility state.
    #[must_use]
    pub const fn visual_mesh_mask(&self) -> &UnitVisualMeshMask {
        &self.visual_mesh_mask
    }

    /// Return the simulation-owned whole-model opacity.
    #[must_use]
    pub const fn visual_opacity(&self) -> f32 {
        self.visual_opacity
    }

    pub(crate) fn set_visual_opacity(&mut self, opacity: f32) {
        self.visual_opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
    }

    pub(crate) fn hide_visual_meshes(&mut self, names: &[String]) {
        self.visual_mesh_mask.hide(names);
    }

    pub(crate) fn set_visual_component_visible(&mut self, name: &str, visible: bool) {
        self.visual_mesh_mask
            .set_component_visibility(name, visible);
    }

    pub(crate) fn set_cloak_mesh_sections(&mut self, cloaked: bool) {
        self.visual_mesh_mask.set_section_visibility(0, !cloaked);
        self.visual_mesh_mask.set_section_visibility(1, cloaked);
    }

    pub(crate) fn detached_damage_part(
        &self,
        id: EntityId,
        player_id: PlayerId,
        profile: &ThrownDamagePart,
        ground_height: f32,
    ) -> Self {
        let mut part = Self::new(id, player_id);
        part.proto_object_id = self.proto_object_id;
        part.proto_object_name.clone_from(&self.proto_object_name);
        part.logical_proto_object_name
            .clone_from(&self.logical_proto_object_name);
        let variation = self
            .object_state
            .visual_variation_index()
            .map_or(-1, |index| i32::try_from(index).unwrap_or(i32::MAX));
        part.object_state.set_visual_variation_index(variation);
        part.base.set_position(self.base.position);
        part.base.set_forward(self.base.forward);
        part.base.set_selectable(false);
        part.set_auto_attackable(false);
        part.set_invulnerable(true);
        part.speed = 0.0;
        part.acceleration = 0.0;
        part.obstruction_half_extents = glam::Vec3::ZERO;
        part.physics = Some(PhysicsBody::dynamic_replacement(
            profile.material(),
            profile.collider(),
            ground_height,
            self.base.position.y,
        ));
        part.visual_mesh_mask.show_only(profile.mesh_names());
        part.visual_opacity = 1.0;
        part
    }

    pub(crate) fn hash_visual_state(&self, checksum: &mut SyncChecksum) {
        self.visual_mesh_mask.hash_state(checksum);
        checksum.hash_f32(self.visual_opacity);
    }
}

fn normalized<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut values = names
        .into_iter()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
}

fn hash_names(checksum: &mut SyncChecksum, names: &[String]) {
    checksum.hash_u32(u32::try_from(names.len()).unwrap_or(u32::MAX));
    for name in names {
        checksum.hash_u32(u32::try_from(name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(name.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityClass;
    use crate::gameplay::ThrownDamagePart;
    use crate::physics::{BoxCollider, PhysicsMaterial};
    use glam::Vec3;

    #[test]
    fn detached_part_copies_visual_identity_and_owns_exclusive_mesh_mask() {
        let id = EntityId::new(EntityClass::Unit, 1);
        let mut source = Unit::new(id, 2);
        source.proto_object_id = 42;
        source.proto_object_name = "vehicle".to_owned();
        source.object_state.set_visual_variation_index(3);
        source.base.set_position(Vec3::new(1.0, 2.0, 3.0));
        let profile = ThrownDamagePart::new(
            vec!["Panel".to_owned()],
            BoxCollider::new(Vec3::ONE, Vec3::Y),
            PhysicsMaterial::default(),
            1.0,
        )
        .unwrap();

        let part =
            source.detached_damage_part(EntityId::new(EntityClass::Unit, 2), 1, &profile, 0.0);

        assert_eq!(part.proto_object_name, "vehicle");
        assert_eq!(part.object_state.visual_variation_index(), Some(3));
        assert_eq!(part.visual_mesh_mask().only(), &["panel"]);
        assert!(part.visual_mesh_mask().section_overrides().is_empty());
        assert_eq!(part.visual_opacity().to_bits(), 1.0_f32.to_bits());
        assert!(!part.base.is_selectable());
        assert!(part.is_invulnerable());
        assert!(part.physics.is_some());
    }

    #[test]
    fn component_visibility_is_case_insensitive_and_reversible() {
        let mut unit = Unit::default();
        unit.set_visual_component_visible(" Shield ", false);
        let hidden_revision = unit.visual_mesh_mask().revision();

        assert!(!unit.visual_mesh_mask().is_component_visible("SHIELD"));
        assert_eq!(unit.visual_mesh_mask().hidden_components(), &["shield"]);
        unit.set_visual_component_visible("shield", false);
        assert_eq!(unit.visual_mesh_mask().revision(), hidden_revision);

        unit.set_visual_component_visible("sHiElD", true);
        assert!(unit.visual_mesh_mask().is_component_visible("shield"));
        assert!(unit.visual_mesh_mask().revision() > hidden_revision);
    }

    #[test]
    fn cloak_projects_retail_mesh_mask_bits() {
        let mut unit = Unit::default();
        unit.set_cloak_mesh_sections(true);
        assert_eq!(
            unit.visual_mesh_mask().section_overrides(),
            &[Some(false), Some(true)]
        );

        unit.set_cloak_mesh_sections(false);
        assert_eq!(
            unit.visual_mesh_mask().section_overrides(),
            &[Some(true), Some(false)]
        );
    }
}
