//! Retail per-unit runtime data scalars.

use super::Unit;

/// Selector used by retail's `ModifyDataScalar` trigger effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UnitDataScalar {
    Accuracy = 0,
    WorkRate = 1,
    Damage = 2,
    LineOfSight = 3,
    Velocity = 4,
    WeaponRange = 5,
    DamageTaken = 6,
}

impl UnitDataScalar {
    /// Resolve either the editor spelling or the serialized integer value.
    #[must_use]
    pub fn from_trigger_value(value: &str) -> Option<Self> {
        match value.trim() {
            value if value.eq_ignore_ascii_case("Accuracy") || value == "0" => Some(Self::Accuracy),
            value if value.eq_ignore_ascii_case("WorkRate") || value == "1" => Some(Self::WorkRate),
            value if value.eq_ignore_ascii_case("Damage") || value == "2" => Some(Self::Damage),
            value
                if value.eq_ignore_ascii_case("LOS")
                    || value.eq_ignore_ascii_case("LineOfSight")
                    || value == "3" =>
            {
                Some(Self::LineOfSight)
            }
            value if value.eq_ignore_ascii_case("Velocity") || value == "4" => Some(Self::Velocity),
            value if value.eq_ignore_ascii_case("WeaponRange") || value == "5" => {
                Some(Self::WeaponRange)
            }
            value if value.eq_ignore_ascii_case("DamageTaken") || value == "6" => {
                Some(Self::DamageTaken)
            }
            _ => None,
        }
    }
}

impl Unit {
    /// Read one live scalar exactly as retail exposes it for synchronization.
    #[must_use]
    pub const fn data_scalar(&self, scalar: UnitDataScalar) -> f32 {
        match scalar {
            UnitDataScalar::Accuracy => self.accuracy_scalar,
            UnitDataScalar::WorkRate => self.work_rate_scalar,
            UnitDataScalar::Damage => self.damage_multiplier,
            UnitDataScalar::LineOfSight => self.line_of_sight_scalar,
            UnitDataScalar::Velocity => self.velocity_scalar,
            UnitDataScalar::WeaponRange => self.weapon_range_scalar,
            UnitDataScalar::DamageTaken => self.damage_taken_multiplier,
        }
    }

    /// Replace one live scalar, matching retail's non-adjusting operation.
    pub fn set_data_scalar(&mut self, scalar: UnitDataScalar, value: f32) {
        *self.data_scalar_mut(scalar) = value;
    }

    /// Multiply one live scalar, matching retail's adjusting operation.
    pub fn adjust_data_scalar(&mut self, scalar: UnitDataScalar, adjustment: f32) {
        *self.data_scalar_mut(scalar) *= adjustment;
    }

    pub(crate) fn modify_data_scalar(&mut self, scalar: UnitDataScalar, value: f32, adjust: bool) {
        if adjust {
            self.adjust_data_scalar(scalar, value);
        } else {
            self.set_data_scalar(scalar, value);
        }
    }

    fn data_scalar_mut(&mut self, scalar: UnitDataScalar) -> &mut f32 {
        match scalar {
            UnitDataScalar::Accuracy => &mut self.accuracy_scalar,
            UnitDataScalar::WorkRate => &mut self.work_rate_scalar,
            UnitDataScalar::Damage => &mut self.damage_multiplier,
            UnitDataScalar::LineOfSight => &mut self.line_of_sight_scalar,
            UnitDataScalar::Velocity => &mut self.velocity_scalar,
            UnitDataScalar::WeaponRange => &mut self.weapon_range_scalar,
            UnitDataScalar::DamageTaken => &mut self.damage_taken_multiplier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;
    use crate::entity_id::{EntityClass, EntityId};
    use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
    use glam::Vec3;

    #[test]
    fn trigger_names_and_wire_values_resolve_exact_selectors() {
        assert_eq!(
            UnitDataScalar::from_trigger_value("DamageTaken"),
            Some(UnitDataScalar::DamageTaken)
        );
        assert_eq!(
            UnitDataScalar::from_trigger_value("los"),
            Some(UnitDataScalar::LineOfSight)
        );
        assert_eq!(
            UnitDataScalar::from_trigger_value("5"),
            Some(UnitDataScalar::WeaponRange)
        );
        assert_eq!(UnitDataScalar::from_trigger_value("unknown"), None);
    }

    #[test]
    fn set_and_adjust_match_retail_assignment_and_multiplication() {
        let mut unit = Unit::default();
        unit.set_data_scalar(UnitDataScalar::Velocity, 2.0);
        unit.adjust_data_scalar(UnitDataScalar::Velocity, 0.25);
        assert!((unit.velocity_scalar - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn velocity_scalar_controls_kinematic_and_physics_driven_units() {
        let id = EntityId::new(EntityClass::Unit, 0);
        let mut kinematic = Unit::new(id, 1);
        kinematic.speed = 10.0;
        kinematic.velocity_scalar = 0.5;
        assert!(kinematic.move_to(Vec3::new(100.0, 0.0, 0.0)));
        kinematic.update(1.0);
        assert!((kinematic.base.position.x - 5.0).abs() < f32::EPSILON);

        let mut physical = Unit::new(id, 1);
        physical.velocity_scalar = 0.5;
        physical.physics = Some(PhysicsBody::ground_vehicle(
            PhysicsMaterial::default(),
            BoxCollider::new(Vec3::ONE, Vec3::ZERO),
            0.0,
            10.0,
            100.0,
            360.0,
        ));
        assert!(physical.move_to(Vec3::new(100.0, 0.0, 0.0)));
        physical.update(0.05);
        assert!(physical.base.velocity.x > 0.0);
        assert!(physical.base.velocity.length() <= 5.0 + f32::EPSILON);
    }
}
