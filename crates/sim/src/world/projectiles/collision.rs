//! Retail projectile obstruction, terrain impact, and damage resolution.

use super::super::World;
use crate::entities::projectiles::{ProjectileStep, ProjectileTargetMotion};
use crate::entities::{Projectile, Unit};
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, ProjectileCollisionTraits};
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::world::combat::AttackDamage;
use glam::Vec3;

const INTERSECTION_EPSILON: f32 = 0.000_001;
const IMPACT_PENETRATION: f32 = 0.01;
const TERRAIN_IMPACT_OFFSET: f32 = 0.25;

#[derive(Debug, Clone, Copy)]
struct ProjectileCollision {
    position: Vec3,
    primary_target_id: Option<EntityId>,
}

#[derive(Debug, Clone)]
struct ProjectileImpact {
    source_id: EntityId,
    source_player_id: PlayerId,
    primary_target_id: Option<EntityId>,
    position: Vec3,
    direction: Vec3,
    damage: f32,
    weapon_type: Option<String>,
    area_damage: Option<crate::gameplay::AreaDamageProfile>,
}

enum ProjectileCollisionOutcome {
    Retained(Option<ProjectileImpact>),
    Finished(Option<ProjectileImpact>),
}

impl World {
    pub(in crate::world) fn update_projectiles(
        &mut self,
        dt: f32,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let intercept_distance = gameplay.map_or(0.0, GameplayCatalog::track_intercept_distance);
        let projectile_ids = self.projectiles.ids().collect::<Vec<_>>();
        let mut impacts = Vec::new();
        let mut finished = Vec::new();
        for projectile_id in projectile_ids {
            self.initialize_projectile_follow_ground_height(projectile_id);
            let live_target = self.live_projectile_target_motion(projectile_id);
            let clear_of_launcher = self.projectile_is_clear_of_launcher(projectile_id);
            let Some((previous, step)) = self.advance_projectile(
                projectile_id,
                dt,
                live_target,
                intercept_distance,
                clear_of_launcher,
            ) else {
                continue;
            };
            self.apply_projectile_tracking_ground_avoidance(
                projectile_id,
                live_target.is_some_and(|target| target.flying),
            );
            let collision =
                self.resolve_projectile_collision(projectile_id, previous, step, gameplay);
            if let Some(collision) = collision {
                match self.handle_projectile_collision(projectile_id, collision, step) {
                    ProjectileCollisionOutcome::Retained(impact) => {
                        impacts.extend(impact);
                    }
                    ProjectileCollisionOutcome::Finished(impact) => {
                        impacts.extend(impact);
                        finished.push(projectile_id);
                    }
                }
            } else if step == ProjectileStep::Expired {
                finished.push(projectile_id);
            }
        }
        for projectile_id in finished {
            let _removed = self.remove_projectile(projectile_id);
        }
        for impact in impacts {
            self.apply_projectile_impact(impact, gameplay);
        }
    }

    fn live_projectile_target_motion(
        &self,
        projectile_id: EntityId,
    ) -> Option<ProjectileTargetMotion> {
        let projectile = self.projectiles.get(projectile_id)?;
        let stuck_to = projectile.stuck_to_unit();
        let target_id = stuck_to.unwrap_or(projectile.target_id);
        let unit = self.units.get(target_id)?;
        if stuck_to.is_none() && !unit.is_attackable() {
            return None;
        }
        Some(ProjectileTargetMotion {
            position: unit.base.position,
            velocity: unit.base.velocity,
            forward: unit.base.forward,
            flying: unit.flying,
        })
    }

    fn initialize_projectile_follow_ground_height(&mut self, projectile_id: EntityId) {
        let Some(position) = self
            .projectiles
            .get(projectile_id)
            .map(|projectile| projectile.base.position)
        else {
            return;
        };
        let terrain_height = self.terrain_height(position, true);
        if let Some(projectile) = self.projectiles.get_mut(projectile_id) {
            projectile.initialize_follow_ground_height(terrain_height);
        }
    }

    fn apply_projectile_tracking_ground_avoidance(
        &mut self,
        projectile_id: EntityId,
        target_flying: bool,
    ) {
        let Some(position) = self
            .projectiles
            .get(projectile_id)
            .map(|projectile| projectile.base.position)
        else {
            return;
        };
        let terrain_height = self.terrain_height(position, true);
        if let Some(projectile) = self.projectiles.get_mut(projectile_id) {
            let _adjusted =
                projectile.apply_tracking_ground_avoidance(terrain_height, target_flying);
        }
    }

    fn advance_projectile(
        &mut self,
        projectile_id: EntityId,
        dt: f32,
        live_target: Option<ProjectileTargetMotion>,
        intercept_distance: f32,
        clear_of_launcher: bool,
    ) -> Option<(Vec3, ProjectileStep)> {
        let (projectiles, sim_rng) = (&mut self.projectiles, &mut self.sim_rng);
        let projectile = projectiles.get_mut(projectile_id)?;
        let previous = projectile.base.position;
        let step = projectile.advance_authoritative(
            dt,
            live_target,
            intercept_distance,
            clear_of_launcher,
            sim_rng,
        );
        Some((previous, step))
    }

    fn projectile_is_clear_of_launcher(&self, projectile_id: EntityId) -> bool {
        let Some(projectile) = self.projectiles.get(projectile_id) else {
            return false;
        };
        if projectile.has_cleared_launcher() {
            return true;
        }
        let Some(source) = self.units.get(projectile.source_id) else {
            return true;
        };
        let radius = source.obstruction_radius();
        let offset = projectile.base.position - source.base.position;
        offset.x * offset.x + offset.z * offset.z >= radius * radius
    }

    fn handle_projectile_collision(
        &mut self,
        projectile_id: EntityId,
        collision: ProjectileCollision,
        step: ProjectileStep,
    ) -> ProjectileCollisionOutcome {
        let Some(projectile) = self.projectiles.get(projectile_id) else {
            return ProjectileCollisionOutcome::Finished(None);
        };
        if projectile.has_timed_lifecycle()
            && matches!(step, ProjectileStep::Flying | ProjectileStep::Impact)
        {
            return ProjectileCollisionOutcome::Retained(
                self.settle_timed_projectile(projectile_id, collision),
            );
        }
        let expire_without_explosion = step == ProjectileStep::Expired
            && projectile.expires_on_timer()
            && !projectile.explodes_on_timer();
        let should_damage = !expire_without_explosion || collision.primary_target_id.is_some();
        let impact = self
            .projectiles
            .get_mut(projectile_id)
            .and_then(|projectile| {
                projectile.base.position = collision.position;
                projectile.base.kill();
                should_damage.then(|| projectile_impact(projectile, collision.primary_target_id))
            });
        ProjectileCollisionOutcome::Finished(impact)
    }

    fn settle_timed_projectile(
        &mut self,
        projectile_id: EntityId,
        collision: ProjectileCollision,
    ) -> Option<ProjectileImpact> {
        let target_transform = collision.primary_target_id.and_then(|unit_id| {
            self.units
                .get(unit_id)
                .map(|unit| (unit_id, unit.base.position, unit.base.forward))
        });
        let projectile = self.projectiles.get_mut(projectile_id)?;
        if projectile.is_sticky()
            && let Some((unit_id, unit_position, unit_forward)) = target_transform
        {
            projectile.stick_to_unit(unit_id, collision.position, unit_position, unit_forward);
        } else {
            projectile.rest_at(collision.position);
        }
        (projectile.expires_on_timer() && collision.primary_target_id.is_some())
            .then(|| projectile_impact(projectile, collision.primary_target_id))
    }

    fn resolve_projectile_collision(
        &self,
        projectile_id: EntityId,
        previous: Vec3,
        step: ProjectileStep,
        gameplay: Option<&GameplayCatalog>,
    ) -> Option<ProjectileCollision> {
        let projectile = self.projectiles.get(projectile_id)?;
        let current = projectile.base.position;
        if !projectile.is_flying() {
            return (step == ProjectileStep::Detonate).then(|| ProjectileCollision {
                position: current,
                primary_target_id: projectile.stuck_to_unit(),
            });
        }
        if previous.distance_squared(current) > INTERSECTION_EPSILON {
            if let Some(collision) =
                self.first_unit_collision(projectile, previous, current, gameplay)
            {
                return Some(collision);
            }
            if (projectile.is_close_to_target() || projectile.tracking || projectile.is_tumbling())
                && let Some(mut position) = self.projectile_terrain_intersection(previous, current)
            {
                position.y += TERRAIN_IMPACT_OFFSET;
                return Some(ProjectileCollision {
                    position,
                    primary_target_id: None,
                });
            }
        }
        matches!(step, ProjectileStep::Impact | ProjectileStep::Detonate).then(|| {
            let primary_target_id = if step == ProjectileStep::Detonate {
                projectile.stuck_to_unit()
            } else {
                self.fallback_projectile_target(projectile, gameplay)
            };
            ProjectileCollision {
                position: current,
                primary_target_id,
            }
        })
    }

    fn first_unit_collision(
        &self,
        projectile: &Projectile,
        start: Vec3,
        end: Vec3,
        gameplay: Option<&GameplayCatalog>,
    ) -> Option<ProjectileCollision> {
        let close_to_target = projectile.is_close_to_target();
        let mut nearest: Option<(f32, EntityId, bool)> = None;
        for (unit_id, unit) in self.units.iter() {
            if !self.is_projectile_collision_candidate(
                projectile,
                unit_id,
                unit,
                close_to_target,
                gameplay,
            ) {
                continue;
            }
            let external_shield = unit.is_external_shield();
            let fraction = if external_shield {
                external_shield_entry_fraction(projectile.initial_position(), start, end, unit)
            } else {
                let (center, half_extents) = unit.simulation_bounds();
                segment_aabb_entry_fraction(start, end, center, half_extents)
            };
            let Some(fraction) = fraction else {
                continue;
            };
            if nearest.is_none_or(|current| match fraction.total_cmp(&current.0) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => unit_id < current.1,
                std::cmp::Ordering::Greater => false,
            }) {
                nearest = Some((fraction, unit_id, external_shield));
            }
        }
        let (fraction, unit_id, external_shield) = nearest?;
        Some(ProjectileCollision {
            position: if external_shield {
                end
            } else {
                penetrated_impact_position(start, end, fraction)
            },
            primary_target_id: Some(unit_id),
        })
    }

    fn is_projectile_collision_candidate(
        &self,
        projectile: &Projectile,
        unit_id: EntityId,
        unit: &Unit,
        close_to_target: bool,
        gameplay: Option<&GameplayCatalog>,
    ) -> bool {
        if !unit.is_attackable() || (unit_id == projectile.source_id && !projectile.self_damage()) {
            return false;
        }
        let traits = projectile_collision_traits(gameplay, unit);
        if (gameplay.is_some() && !traits.is_known()) || traits.is_neutral() {
            return false;
        }
        if !traits.is_projectile_obstructable() {
            if !unit.is_external_shield()
                && !close_to_target
                && unit_id != projectile.target_id
                && !projectile.self_damage()
            {
                return false;
            }
            if !unit.is_object_type("Cover")
                && !projectile.friendly_fire()
                && !self.players_are_enemies(projectile.base.player_id, unit.base.player_id)
                && unit.base.player_id != GAIA_PLAYER
            {
                return false;
            }
        }
        projectile.collides_with_all_units() || !traits.targets_foot_of_unit()
    }

    fn fallback_projectile_target(
        &self,
        projectile: &Projectile,
        gameplay: Option<&GameplayCatalog>,
    ) -> Option<EntityId> {
        let target = self.units.get(projectile.target_id)?;
        let (center, half_extents) = target.simulation_bounds();
        let inside_target = (projectile.base.position - center)
            .abs()
            .cmple(half_extents.abs())
            .all();
        (inside_target
            && self.is_projectile_collision_candidate(
                projectile,
                projectile.target_id,
                target,
                true,
                gameplay,
            ))
        .then_some(projectile.target_id)
    }

    fn apply_projectile_impact(
        &mut self,
        impact: ProjectileImpact,
        gameplay: Option<&GameplayCatalog>,
    ) {
        self.apply_attack_damage(
            &AttackDamage {
                attacker_id: impact.source_id,
                attacker_player_id: impact.source_player_id,
                primary_target_id: impact.primary_target_id,
                ground_zero: impact.position,
                direction: impact.direction,
                damage: impact.damage,
                weapon_type: impact.weapon_type,
                area_damage: impact.area_damage,
            },
            gameplay,
        );
    }
}

fn projectile_collision_traits(
    gameplay: Option<&GameplayCatalog>,
    unit: &Unit,
) -> ProjectileCollisionTraits {
    gameplay.map_or_else(ProjectileCollisionTraits::default, |catalog| {
        catalog.projectile_collision_traits(&unit.proto_object_name)
    })
}

fn projectile_impact(
    projectile: &Projectile,
    primary_target_id: Option<EntityId>,
) -> ProjectileImpact {
    ProjectileImpact {
        source_id: projectile.source_id,
        source_player_id: projectile.base.player_id,
        primary_target_id,
        position: projectile.base.position,
        direction: projectile.base.velocity,
        damage: projectile.damage,
        weapon_type: projectile.weapon_type.clone(),
        area_damage: projectile.area_damage,
    }
}

fn penetrated_impact_position(start: Vec3, end: Vec3, fraction: f32) -> Vec3 {
    let segment = end - start;
    let length = segment.length();
    if length <= INTERSECTION_EPSILON {
        return start;
    }
    start + segment * (fraction + IMPACT_PENETRATION / length).min(1.0)
}

fn segment_aabb_entry_fraction(
    start: Vec3,
    end: Vec3,
    center: Vec3,
    half_extents: Vec3,
) -> Option<f32> {
    if !start.is_finite() || !end.is_finite() || !center.is_finite() || !half_extents.is_finite() {
        return None;
    }
    let start = start.to_array();
    let direction = (end - Vec3::from_array(start)).to_array();
    let minimum = (center - half_extents.abs()).to_array();
    let maximum = (center + half_extents.abs()).to_array();
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for axis in 0..3 {
        if direction[axis].abs() <= INTERSECTION_EPSILON {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return None;
            }
            continue;
        }
        let inverse = direction[axis].recip();
        let first = (minimum[axis] - start[axis]) * inverse;
        let second = (maximum[axis] - start[axis]) * inverse;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return None;
        }
    }
    Some(near)
}

fn external_shield_entry_fraction(
    initial: Vec3,
    start: Vec3,
    end: Vec3,
    shield: &Unit,
) -> Option<f32> {
    let center = shield.base.position;
    let radii = shield.obstruction_half_extents.abs();
    let radius_squared = radii.x.mul_add(radii.x, radii.z * radii.z);
    if !center.is_finite()
        || !radii.is_finite()
        || !radius_squared.is_finite()
        || radius_squared <= INTERSECTION_EPSILON
    {
        return None;
    }
    let launched_inside = external_shield_contains(initial, center, radii.y, radius_squared);
    let is_wall_or_base = ["WallShield", "_WallShield", "BaseShield", "_BaseShield"]
        .iter()
        .any(|object_type| shield.is_object_type(object_type));
    if launched_inside && !is_wall_or_base {
        return None;
    }

    let direction = end - start;
    let relative_start = start - center;
    let quadratic = direction.length_squared();
    if quadratic <= INTERSECTION_EPSILON {
        return external_shield_contains(start, center, radii.y, radius_squared).then_some(0.0);
    }
    let half_linear = relative_start.dot(direction);
    let constant = relative_start.length_squared() - radius_squared;
    let discriminant = half_linear.mul_add(half_linear, -quadratic * constant);
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let mut near = (-half_linear - root) / quadratic;
    let mut far = (-half_linear + root) / quadratic;
    if direction.y.abs() <= INTERSECTION_EPSILON {
        if relative_start.y.abs() > radii.y {
            return None;
        }
    } else {
        let first = (-radii.y - relative_start.y) / direction.y;
        let second = (radii.y - relative_start.y) / direction.y;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
    }
    near = near.max(0.0);
    far = far.min(1.0);
    (near <= far).then_some(near)
}

fn external_shield_contains(point: Vec3, center: Vec3, radius_y: f32, radius_squared: f32) -> bool {
    (point.y - center.y).abs() <= radius_y && point.distance_squared(center) <= radius_squared
}

#[cfg(test)]
mod tests;
