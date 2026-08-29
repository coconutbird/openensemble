//! Retail-compatible primary-target and splash-pool damage distribution.

use super::World;
use crate::entities::Unit;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AreaDamageProfile, GameplayCatalog};
use crate::player::{GAIA_PLAYER, PlayerId};
use glam::Vec3;

const DAMAGE_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone)]
pub(crate) struct AttackDamage {
    pub attacker_id: EntityId,
    pub attacker_player_id: PlayerId,
    pub primary_target_id: Option<EntityId>,
    pub ground_zero: Vec3,
    pub direction: Vec3,
    pub damage: f32,
    pub weapon_type: Option<String>,
    pub area_damage: Option<AreaDamageProfile>,
}

#[derive(Debug, Clone, Copy)]
struct AreaCandidate {
    id: EntityId,
    distance: f32,
    damage: f32,
}

#[derive(Debug, Default)]
struct AreaCandidates {
    uncapped_gaia: Vec<AreaCandidate>,
    capped: Vec<AreaCandidate>,
}

#[derive(Debug, Clone, Copy, Default)]
struct PrimaryDamage {
    dealt: f32,
    killed: bool,
}

#[derive(Debug, Clone, Copy)]
struct ExternalShieldVolume {
    original_primary_id: EntityId,
    center: Vec3,
    radius_x: f32,
    radius_y: f32,
}

impl ExternalShieldVolume {
    fn contains(self, center: Vec3) -> bool {
        center.y >= self.center.y - self.radius_y
            && center.y <= self.center.y + self.radius_y
            && self.center.distance_squared(center) <= self.radius_x * self.radius_x
    }
}

impl World {
    pub(crate) fn apply_attack_damage(
        &mut self,
        attack: &AttackDamage,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        let Some(profile) = attack.area_damage else {
            return attack.primary_target_id.map_or(0.0, |target_id| {
                self.apply_directional_weapon_damage(
                    attack.attacker_player_id,
                    target_id,
                    attack.damage,
                    attack.weapon_type.as_deref(),
                    attack.direction,
                    gameplay,
                )
            });
        };
        self.apply_area_damage(attack, profile, gameplay)
    }

    fn apply_area_damage(
        &mut self,
        attack: &AttackDamage,
        profile: AreaDamageProfile,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        let primary = self.apply_primary_area_damage(attack, profile, gameplay);
        let splash_pool = attack.primary_target_id.map_or(attack.damage, |_| {
            (1.0 - profile.primary_target_factor) * attack.damage
        });
        let external_shield = self.primary_external_shield_volume(attack.primary_target_id);
        let mut candidates = self.area_damage_candidates(attack, profile, primary, external_shield);
        let mut total = primary.dealt;
        total += self.apply_uncapped_gaia_damage(
            &candidates.uncapped_gaia,
            attack.attacker_player_id,
            attack.weapon_type.as_deref(),
            gameplay,
        );
        if profile.linear_damage {
            candidates
                .capped
                .sort_by(|left, right| left.distance.total_cmp(&right.distance));
            total += self.apply_linear_area_damage(
                &candidates.capped,
                splash_pool,
                attack.attacker_player_id,
                attack.weapon_type.as_deref(),
                gameplay,
            );
        } else {
            total += self.apply_capped_area_damage(
                &candidates.capped,
                splash_pool,
                attack.attacker_player_id,
                attack.weapon_type.as_deref(),
                gameplay,
            );
        }
        total
    }

    fn apply_primary_area_damage(
        &mut self,
        attack: &AttackDamage,
        profile: AreaDamageProfile,
        gameplay: Option<&GameplayCatalog>,
    ) -> PrimaryDamage {
        let Some(target_id) = attack.primary_target_id else {
            return PrimaryDamage::default();
        };
        let receiving_target_id = self.resolve_damage_target(target_id);
        let was_alive = self
            .units
            .get(receiving_target_id)
            .is_some_and(Entity::is_alive);
        let dealt = self.apply_weapon_damage(
            attack.attacker_player_id,
            target_id,
            attack.damage * profile.primary_target_factor,
            attack.weapon_type.as_deref(),
            gameplay,
        );
        let killed = was_alive
            && !self
                .units
                .get(receiving_target_id)
                .is_some_and(Entity::is_alive);
        PrimaryDamage { dealt, killed }
    }

    fn area_damage_candidates(
        &self,
        attack: &AttackDamage,
        profile: AreaDamageProfile,
        primary: PrimaryDamage,
        external_shield: Option<ExternalShieldVolume>,
    ) -> AreaCandidates {
        let mut candidates = AreaCandidates::default();
        for (id, unit) in self.units.iter() {
            if !self.is_area_damage_candidate(
                id,
                unit,
                attack,
                profile,
                primary.killed,
                external_shield,
            ) {
                continue;
            }
            let (center, half_extents) = unit.simulation_bounds();
            let mut distance_squared =
                point_aabb_distance_squared(attack.ground_zero, center, half_extents, profile);
            if distance_squared < 0.01
                || (attack.primary_target_id == Some(id) && primary.dealt > DAMAGE_EPSILON)
            {
                distance_squared = 0.0;
            }
            if distance_squared > profile.radius * profile.radius {
                continue;
            }
            let distance = distance_squared.sqrt();
            let candidate = AreaCandidate {
                id,
                distance,
                damage: area_distance_factor(distance, profile) * attack.damage,
            };
            if unit.base.player_id == GAIA_PLAYER && !unit.is_object_type("Cover") {
                candidates.uncapped_gaia.push(candidate);
            } else {
                candidates.capped.push(candidate);
            }
        }
        candidates
    }

    fn is_area_damage_candidate(
        &self,
        id: EntityId,
        unit: &Unit,
        attack: &AttackDamage,
        profile: AreaDamageProfile,
        killed_primary: bool,
        external_shield: Option<ExternalShieldVolume>,
    ) -> bool {
        if id == attack.attacker_id
            || !unit.is_attackable()
            || (killed_primary && attack.primary_target_id == Some(id))
        {
            return false;
        }
        let (center, half_extents) = unit.simulation_bounds();
        if external_shield
            .is_some_and(|shield| id != shield.original_primary_id && shield.contains(center))
        {
            return false;
        }
        if profile.linear_damage
            && !segment_intersects_aabb(
                attack.ground_zero,
                linear_area_end(attack, profile),
                center,
                half_extents,
            )
        {
            return false;
        }
        profile.friendly_fire
            || (unit.base.player_id != attack.attacker_player_id
                && !self.players_are_allied(unit.base.player_id, attack.attacker_player_id))
    }

    fn primary_external_shield_volume(
        &self,
        primary_target_id: Option<EntityId>,
    ) -> Option<ExternalShieldVolume> {
        let original_primary_id = primary_target_id?;
        let shield_id = self.resolve_damage_target(original_primary_id);
        let shield = self
            .units
            .get(shield_id)
            .filter(|unit| unit.is_external_shield())?;
        Some(ExternalShieldVolume {
            original_primary_id,
            center: shield.base.position,
            radius_x: shield.obstruction_half_extents.x.abs(),
            radius_y: shield.obstruction_half_extents.y.abs(),
        })
    }

    fn apply_uncapped_gaia_damage(
        &mut self,
        candidates: &[AreaCandidate],
        attacker_player_id: PlayerId,
        weapon_type: Option<&str>,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        candidates
            .iter()
            .map(|candidate| {
                self.apply_weapon_damage(
                    attacker_player_id,
                    candidate.id,
                    candidate.damage,
                    weapon_type,
                    gameplay,
                )
            })
            .sum()
    }

    fn apply_linear_area_damage(
        &mut self,
        candidates: &[AreaCandidate],
        mut splash_pool: f32,
        attacker_player_id: PlayerId,
        weapon_type: Option<&str>,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        let mut total = 0.0;
        for candidate in candidates {
            let dealt = self.apply_weapon_damage(
                attacker_player_id,
                candidate.id,
                splash_pool,
                weapon_type,
                gameplay,
            );
            total += dealt;
            splash_pool -= dealt;
            if splash_pool < DAMAGE_EPSILON {
                break;
            }
        }
        total
    }

    fn apply_capped_area_damage(
        &mut self,
        candidates: &[AreaCandidate],
        splash_pool: f32,
        attacker_player_id: PlayerId,
        weapon_type: Option<&str>,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        let total_damage = candidates
            .iter()
            .map(|candidate| candidate.damage)
            .sum::<f32>();
        let reduction = if total_damage > splash_pool && total_damage != 0.0 {
            splash_pool / total_damage
        } else {
            1.0
        };
        candidates
            .iter()
            .map(|candidate| {
                self.apply_weapon_damage(
                    attacker_player_id,
                    candidate.id,
                    candidate.damage * reduction,
                    weapon_type,
                    gameplay,
                )
            })
            .sum()
    }
}

fn point_aabb_distance_squared(
    point: Vec3,
    center: Vec3,
    half_extents: Vec3,
    profile: AreaDamageProfile,
) -> f32 {
    let mut delta = (point - center).abs() - half_extents;
    if profile.ignores_y_axis {
        delta.y = 0.0;
    }
    delta.max(Vec3::ZERO).length_squared()
}

fn linear_area_end(attack: &AttackDamage, profile: AreaDamageProfile) -> Vec3 {
    attack.ground_zero + attack.direction.normalize_or_zero() * profile.radius
}

fn area_distance_factor(distance: f32, profile: AreaDamageProfile) -> f32 {
    if profile.distance_factor == 0.0 {
        return 1.0;
    }
    let outer_distance = profile.distance_factor * profile.radius;
    if distance <= outer_distance {
        1.0 - (distance / outer_distance) * (1.0 - profile.damage_factor)
    } else {
        (1.0 - (distance - outer_distance) / (profile.radius - outer_distance))
            * profile.damage_factor
    }
}

fn segment_intersects_aabb(start: Vec3, end: Vec3, center: Vec3, half_extents: Vec3) -> bool {
    let start = start.to_array();
    let direction = (end - Vec3::from_array(start)).to_array();
    let minimum = (center - half_extents).to_array();
    let maximum = (center + half_extents).to_array();
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for axis in 0..3 {
        if direction[axis].abs() <= DAMAGE_EPSILON {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return false;
            }
            continue;
        }
        let inverse = direction[axis].recip();
        let first = (minimum[axis] - start[axis]) * inverse;
        let second = (maximum[axis] - start[axis]) * inverse;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::ShieldCoverage;

    fn profile() -> AreaDamageProfile {
        AreaDamageProfile {
            radius: 4.0,
            primary_target_factor: 0.0,
            distance_factor: 0.0,
            damage_factor: 0.0,
            linear_damage: false,
            ignores_y_axis: false,
            friendly_fire: false,
        }
    }

    fn combat_world() -> (World, EntityId) {
        let mut world = World::new();
        world.init_players(3);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.get_player_mut(3).unwrap().team_id = 1;
        world.configure_standard_team_relations();
        let attacker_id = world.create_unit_at(1, Vec3::new(-5.0, 0.0, 0.0));
        (world, attacker_id)
    }

    fn attack(
        attacker_id: EntityId,
        primary_target_id: Option<EntityId>,
        profile: AreaDamageProfile,
    ) -> AttackDamage {
        AttackDamage {
            attacker_id,
            attacker_player_id: 1,
            primary_target_id,
            ground_zero: Vec3::ZERO,
            direction: Vec3::X,
            damage: 100.0,
            weapon_type: None,
            area_damage: Some(profile),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.000_1,
            "expected {expected}, got {actual}"
        );
    }

    fn front_shield_target(world: &mut World) -> EntityId {
        let target_id = world.create_unit_at(2, Vec3::ZERO);
        let target = world.get_unit_mut(target_id).expect("target");
        target.base.set_forward(Vec3::Z);
        target.shields.configure(ShieldCoverage::FrontHalf, 20.0);
        target.shields.set_current(20.0);
        target_id
    }

    #[test]
    fn direct_hits_use_facing_while_authored_aoe_is_nondirectional() {
        let (mut world, attacker_id) = combat_world();
        let rear_target_id = front_shield_target(&mut world);
        world.apply_attack_damage(
            &AttackDamage {
                attacker_id,
                attacker_player_id: 1,
                primary_target_id: Some(rear_target_id),
                ground_zero: Vec3::ZERO,
                direction: Vec3::Z,
                damage: 10.0,
                weapon_type: None,
                area_damage: None,
            },
            None,
        );
        let rear_target = world.get_unit(rear_target_id).expect("rear target");
        assert_close(rear_target.hitpoints, 90.0);
        assert_close(rear_target.shields.current, 20.0);

        let front_target_id = front_shield_target(&mut world);
        world.apply_attack_damage(
            &AttackDamage {
                attacker_id,
                attacker_player_id: 1,
                primary_target_id: Some(front_target_id),
                ground_zero: Vec3::ZERO,
                direction: Vec3::NEG_Z,
                damage: 10.0,
                weapon_type: None,
                area_damage: None,
            },
            None,
        );
        let front_target = world.get_unit(front_target_id).expect("front target");
        assert_close(front_target.hitpoints, 100.0);
        assert_close(front_target.shields.current, 10.0);

        let area_target_id = front_shield_target(&mut world);
        let mut area = profile();
        area.primary_target_factor = 1.0;
        world.apply_attack_damage(
            &AttackDamage {
                attacker_id,
                attacker_player_id: 1,
                primary_target_id: Some(area_target_id),
                ground_zero: Vec3::ZERO,
                direction: Vec3::Z,
                damage: 10.0,
                weapon_type: None,
                area_damage: Some(area),
            },
            None,
        );
        let area_target = world.get_unit(area_target_id).expect("AOE target");
        assert_close(area_target.hitpoints, 100.0);
        assert_close(area_target.shields.current, 10.0);
    }

    #[test]
    fn nonlinear_pool_reserves_primary_damage_and_excludes_allies() {
        let (mut world, attacker_id) = combat_world();
        let primary_id = world.create_unit_at(2, Vec3::ZERO);
        let secondary_id = world.create_unit_at(2, Vec3::X);
        let ally_id = world.create_unit_at(3, Vec3::new(2.0, 0.0, 0.0));
        let gaia_id = world.create_unit_at(GAIA_PLAYER, Vec3::new(3.0, 0.0, 0.0));
        let far_id = world.create_unit_at(2, Vec3::new(5.0, 0.0, 0.0));
        let mut area = profile();
        area.primary_target_factor = 0.25;

        world.apply_attack_damage(&attack(attacker_id, Some(primary_id), area), None);

        assert_close(world.get_unit(primary_id).unwrap().hitpoints, 37.5);
        assert_close(world.get_unit(secondary_id).unwrap().hitpoints, 62.5);
        assert_close(world.get_unit(ally_id).unwrap().hitpoints, 100.0);
        assert_close(world.get_unit(far_id).unwrap().hitpoints, 100.0);
        assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 100.0);
        assert_close(world.get_unit(gaia_id).unwrap().hitpoints, 0.0);
    }

    #[test]
    fn proxy_external_shield_excludes_other_units_inside_its_aoe_volume() {
        let (mut world, attacker_id) = combat_world();
        let protected_squad = world.create_squad(2);
        let primary_id = world.create_unit_at(2, Vec3::ZERO);
        assert!(world.attach_unit_to_squad(primary_id, protected_squad));
        let shield_squad = world.create_squad(2);
        let shield_id = world.create_unit_at(2, Vec3::ZERO);
        assert!(world.attach_unit_to_squad(shield_id, shield_squad));
        let shield = world.get_unit_mut(shield_id).unwrap();
        shield.set_external_shield(true);
        shield.obstruction_half_extents = Vec3::new(4.0, 3.0, 4.0);
        world
            .get_squad_mut(protected_squad)
            .unwrap()
            .set_damage_proxy(shield_squad);
        let inside_id = world.create_unit_at(2, Vec3::X * 2.0);
        let outside_id = world.create_unit_at(2, Vec3::X * 6.0);
        let mut area = profile();
        area.radius = 10.0;
        area.primary_target_factor = 0.5;

        world.apply_attack_damage(&attack(attacker_id, Some(primary_id), area), None);

        assert_close(world.get_unit(primary_id).unwrap().hitpoints, 100.0);
        assert_close(world.get_unit(inside_id).unwrap().hitpoints, 100.0);
        assert!(world.get_unit(outside_id).unwrap().hitpoints < 100.0);
        assert!(world.get_unit(shield_id).unwrap().hitpoints < 50.0);
    }

    #[test]
    fn friendly_fire_includes_owned_and_allied_units_but_not_the_attacker() {
        let (mut world, attacker_id) = combat_world();
        let enemy_id = world.create_unit_at(2, Vec3::X);
        let ally_id = world.create_unit_at(3, Vec3::new(2.0, 0.0, 0.0));
        let owned_id = world.create_unit_at(1, Vec3::new(3.0, 0.0, 0.0));
        let mut area = profile();
        area.friendly_fire = true;

        world.apply_attack_damage(&attack(attacker_id, None, area), None);

        assert_close(
            world.get_unit(enemy_id).unwrap().hitpoints,
            100.0 - 100.0 / 3.0,
        );
        assert_close(
            world.get_unit(ally_id).unwrap().hitpoints,
            100.0 - 100.0 / 3.0,
        );
        assert_close(
            world.get_unit(owned_id).unwrap().hitpoints,
            100.0 - 100.0 / 3.0,
        );
        assert_close(world.get_unit(attacker_id).unwrap().hitpoints, 100.0);
    }

    #[test]
    fn linear_pool_is_consumed_nearest_target_first_using_base_damage() {
        let (mut world, attacker_id) = combat_world();
        let near_id = world.create_unit_at(2, Vec3::X);
        world.get_unit_mut(near_id).unwrap().set_max_hitpoints(30.0);
        let far_id = world.create_unit_at(2, Vec3::new(2.0, 0.0, 0.0));
        let mut area = profile();
        area.linear_damage = true;

        world.apply_attack_damage(&attack(attacker_id, None, area), None);

        assert_close(world.get_unit(near_id).unwrap().hitpoints, 0.0);
        assert_close(world.get_unit(far_id).unwrap().hitpoints, 30.0);
    }

    #[test]
    fn authored_two_interval_falloff_and_vertical_ignore_match_retail_math() {
        let mut area = profile();
        area.radius = 10.0;
        area.distance_factor = 0.5;
        area.damage_factor = 0.5;
        assert_close(area_distance_factor(0.0, area), 1.0);
        assert_close(area_distance_factor(5.0, area), 0.5);
        assert_close(area_distance_factor(7.5, area), 0.25);
        assert_close(area_distance_factor(10.0, area), 0.0);

        let center = Vec3::new(0.0, 20.0, 0.0);
        assert_close(
            point_aabb_distance_squared(Vec3::ZERO, center, Vec3::ZERO, area),
            400.0,
        );
        area.ignores_y_axis = true;
        assert_close(
            point_aabb_distance_squared(Vec3::ZERO, center, Vec3::ZERO, area),
            0.0,
        );
    }
}
