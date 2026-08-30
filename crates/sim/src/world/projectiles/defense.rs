//! Retail projectile collision reactions backed by persistent unit actions.

use super::super::World;
use crate::entities::squads::formation_offset_to_local;
use crate::entities::{SquadMode, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{DeflectActionProfile, DodgeActionProfile, GameplayCatalog};
use glam::Vec3;
use num_traits::ToPrimitive;

const DIRECTION_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone)]
struct DefenseTarget {
    proto_object_name: String,
    center: Vec3,
    forward: Vec3,
    flying: bool,
    squad: Option<DefenseSquad>,
}

#[derive(Debug, Clone, Copy)]
struct DefenseSquad {
    position: Vec3,
    forward: Vec3,
    mode: SquadMode,
    last_damage_time: u32,
    frozen: bool,
    joining: bool,
}

#[derive(Debug, Clone, Copy)]
struct DodgeOption {
    direction: Vec3,
    position: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct DeflectAttempt<'a> {
    projectile_id: EntityId,
    target_id: EntityId,
    previous: Vec3,
    impact_position: Vec3,
    trajectory: Vec3,
    damage: f32,
    target: &'a DefenseTarget,
    profile: &'a DeflectActionProfile,
    gameplay: &'a GameplayCatalog,
}

impl World {
    pub(super) fn try_projectile_defense(
        &mut self,
        projectile_id: EntityId,
        target_id: EntityId,
        previous: Vec3,
        impact_position: Vec3,
        trajectory: Vec3,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(target) = self.defense_target(target_id) else {
            return false;
        };
        let Some(profile) = gameplay.projectile_defense(&target.proto_object_name) else {
            return false;
        };
        let Some((dodgeable, deflectable, damage)) =
            self.projectile_reaction_permissions(projectile_id, profile.deflect())
        else {
            return false;
        };
        if !dodgeable && !deflectable {
            return false;
        }
        if let Some(projectile) = self.projectiles.get_mut(projectile_id) {
            projectile.mark_checked_for_defense();
        }
        if deflectable
            && profile.deflect().is_some_and(|deflect| {
                self.try_deflect(DeflectAttempt {
                    projectile_id,
                    target_id,
                    previous,
                    impact_position,
                    trajectory,
                    damage,
                    target: &target,
                    profile: deflect,
                    gameplay,
                })
            })
        {
            return true;
        }
        dodgeable
            && profile.dodge().is_some_and(|dodge| {
                self.try_dodge(projectile_id, target_id, trajectory, &target, dodge)
            })
    }

    fn projectile_reaction_permissions(
        &self,
        projectile_id: EntityId,
        deflect: Option<&DeflectActionProfile>,
    ) -> Option<(bool, bool, f32)> {
        let projectile = self.projectiles.get(projectile_id)?;
        if projectile.checked_for_defense() {
            return None;
        }
        let deflectable =
            deflect.is_some_and(|profile| projectile.is_deflectable(profile.small_arms()));
        Some((projectile.is_dodgeable(), deflectable, projectile.damage))
    }

    fn try_deflect(&mut self, attempt: DeflectAttempt<'_>) -> bool {
        let DeflectAttempt {
            projectile_id,
            target_id,
            previous,
            impact_position,
            trajectory,
            damage,
            target,
            profile,
            gameplay,
        } = attempt;
        if !self.defense_action_enabled(target_id, profile.action_name(), profile.starts_disabled())
            || profile
                .squad_mode()
                .is_some_and(|mode| target.squad.is_none_or(|squad| squad.mode != mode))
        {
            return false;
        }
        let squad_last_damage = target.squad.map_or(0, |squad| squad.last_damage_time);
        let regen_delay_ms = self.deflect_regen_delay_ms(target_id, gameplay);
        let now_ms = self.game_time_ms;
        let Some(state) = self
            .units
            .get_mut(target_id)
            .map(|unit| &mut unit.projectile_defense)
        else {
            return false;
        };
        state.refresh_deflecting(
            now_ms,
            squad_last_damage,
            regen_delay_ms,
            profile.has_shield_visual(),
        );
        if !state.can_begin_deflect(now_ms)
            || (profile.waits_for_dodge_cooldown() && state.dodge_cooldown_active(now_ms))
        {
            return false;
        }
        if !reaction_succeeds(
            &mut self.sim_rng,
            target.forward,
            trajectory,
            profile.max_angle(),
            profile.chance_max(),
            profile.chance_min(),
        ) || !self.units.get_mut(target_id).is_some_and(|unit| {
            unit.projectile_defense.try_deflect(
                now_ms,
                profile.cooldown(),
                damage,
                profile.max_damage(),
            )
        }) {
            return false;
        }
        let (projectiles, rng) = (&mut self.projectiles, &mut self.sim_rng);
        let Some(projectile) = projectiles.get_mut(projectile_id) else {
            return false;
        };
        projectile.deflect_from(previous, impact_position, target.center, rng);
        true
    }

    fn try_dodge(
        &mut self,
        projectile_id: EntityId,
        target_id: EntityId,
        trajectory: Vec3,
        target: &DefenseTarget,
        profile: &DodgeActionProfile,
    ) -> bool {
        let Some(squad) = target.squad else {
            return false;
        };
        if squad.mode == SquadMode::Cover
            || squad.frozen
            || squad.joining
            || !self.defense_action_enabled(
                target_id,
                profile.action_name(),
                profile.starts_disabled(),
            )
        {
            return false;
        }
        let now_ms = self.game_time_ms;
        let Some(state) = self
            .units
            .get_mut(target_id)
            .map(|unit| &mut unit.projectile_defense)
        else {
            return false;
        };
        if !state.can_begin_dodge(now_ms)
            || (profile.waits_for_deflect_cooldown() && state.deflect_cooldown_active(now_ms))
        {
            return false;
        }
        if !reaction_succeeds(
            &mut self.sim_rng,
            target.forward,
            trajectory,
            profile.max_angle(),
            profile.chance_max(),
            profile.chance_min(),
        ) {
            return false;
        }
        let options = self.dodge_options(target_id, target);
        let Some(option) = self.select_dodge_option(&options) else {
            return false;
        };
        if !self.apply_dodge_option(target_id, target.squad, option, profile.physics_impulse()) {
            return false;
        }
        if let Some(unit) = self.units.get_mut(target_id) {
            unit.projectile_defense
                .begin_dodge(projectile_id, now_ms, profile.cooldown());
        }
        if let Some(projectile) = self.projectiles.get_mut(projectile_id) {
            projectile.dodge_target(target.flying);
        }
        true
    }

    fn defense_target(&self, target_id: EntityId) -> Option<DefenseTarget> {
        let unit = self
            .units
            .get(target_id)
            .filter(|unit| unit.is_operational())?;
        let squad = unit.squad_id.and_then(|squad_id| {
            self.squads.get(squad_id).map(|squad| DefenseSquad {
                position: squad.base.position,
                forward: squad.base.forward,
                mode: squad.mode,
                last_damage_time: squad.last_damaged_time,
                frozen: squad.is_cryo_frozen() || unit.is_cryo_frozen(),
                joining: squad.join_target().is_some(),
            })
        });
        Some(DefenseTarget {
            proto_object_name: unit.proto_object_name.clone(),
            center: unit.simulation_center(),
            forward: unit.base.forward,
            flying: unit.flying,
            squad,
        })
    }

    fn defense_action_enabled(
        &self,
        target_id: EntityId,
        action_name: &str,
        starts_disabled: bool,
    ) -> bool {
        let Some(unit) = self.units.get(target_id) else {
            return false;
        };
        let authored_enabled = !starts_disabled;
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        action_name,
                        authored_enabled,
                    )
                });
        unit.actions.is_enabled(action_name, !player_enabled)
    }

    fn deflect_regen_delay_ms(&self, target_id: EntityId, gameplay: &GameplayCatalog) -> u32 {
        let Some(unit) = self.units.get(target_id) else {
            return 0;
        };
        let base =
            self.get_player(unit.base.player_id)
                .map_or(gameplay.shield_regen_delay(), |player| {
                    player
                        .technologies
                        .shield_regen_delay(gameplay.shield_regen_delay())
                });
        seconds_to_millis(base * unit.shields.regen_delay_scalar())
    }

    fn dodge_options(&self, target_id: EntityId, target: &DefenseTarget) -> Vec<DodgeOption> {
        let Some(unit) = self.units.get(target_id) else {
            return Vec::new();
        };
        let forward = horizontal(target.forward).normalize_or(Vec3::Z);
        let right = Vec3::new(forward.z, 0.0, -forward.x);
        let distance = unit.obstruction_radius().max(0.25) * 3.0;
        let directions = [right, right, -right, -right, forward, -forward];
        directions
            .into_iter()
            .filter_map(|direction| {
                let mut position = unit.base.position + direction * distance;
                position.y = self.terrain_height(position, true).unwrap_or(position.y);
                (!self.dodge_position_is_obstructed(target_id, position, unit)).then_some(
                    DodgeOption {
                        direction,
                        position,
                    },
                )
            })
            .collect()
    }

    fn select_dodge_option(&mut self, options: &[DodgeOption]) -> Option<DodgeOption> {
        let maximum = u32::try_from(options.len().checked_sub(1)?).ok()?;
        let index = usize::try_from(self.sim_rng.index(maximum)).ok()?;
        options.get(index).copied()
    }

    fn dodge_position_is_obstructed(
        &self,
        target_id: EntityId,
        position: Vec3,
        target: &Unit,
    ) -> bool {
        let radius = target.obstruction_radius().max(0.25);
        if self.effective_playable_bounds().is_some_and(|bounds| {
            !bounds.contains(position + Vec3::new(radius, 0.0, radius))
                || !bounds.contains(position - Vec3::new(radius, 0.0, radius))
        }) {
            return true;
        }
        self.units.iter().any(|(unit_id, unit)| {
            if unit_id == target_id
                || !unit.is_alive()
                || (unit.base.is_mobile() && !unit.is_building())
            {
                return false;
            }
            let other_radius = unit.obstruction_radius().max(0.25);
            let offset = horizontal(unit.base.position - position);
            offset.length_squared() < (radius + other_radius).powi(2)
        })
    }

    fn apply_dodge_option(
        &mut self,
        target_id: EntityId,
        squad: Option<DefenseSquad>,
        option: DodgeOption,
        physics_impulse: f32,
    ) -> bool {
        let Some(unit) = self.units.get_mut(target_id) else {
            return false;
        };
        if physics_impulse > 0.0 {
            let Some(mass) = unit.physics.as_ref().map(|body| body.material().mass) else {
                return false;
            };
            return unit.apply_impulse(option.direction * physics_impulse * mass);
        }
        unit.base.position = option.position;
        if let Some(squad) = squad {
            unit.formation_offset =
                formation_offset_to_local(squad.forward, option.position - squad.position);
        }
        true
    }
}

fn reaction_succeeds(
    rng: &mut crate::random::SimRandom,
    forward: Vec3,
    trajectory: Vec3,
    maximum_angle: f32,
    chance_max: f32,
    chance_min: f32,
) -> bool {
    if maximum_angle <= 0.0 {
        return false;
    }
    let incoming = -horizontal(trajectory).normalize_or(Vec3::Z);
    let facing = horizontal(forward).normalize_or(Vec3::Z);
    let angle = facing.dot(incoming).clamp(-1.0, 1.0).acos();
    if angle >= maximum_angle {
        return false;
    }
    let chance = chance_max + (chance_min - chance_max) * (angle / maximum_angle);
    rng.range_float(0.0, 1.0) <= chance
}

fn horizontal(vector: Vec3) -> Vec3 {
    let horizontal = Vec3::new(vector.x, 0.0, vector.z);
    if horizontal.length_squared() > DIRECTION_EPSILON {
        horizontal
    } else {
        Vec3::ZERO
    }
}

fn seconds_to_millis(seconds: f32) -> u32 {
    if seconds.is_finite() && seconds > 0.0 {
        (seconds * 1_000.0).to_u32().unwrap_or(u32::MAX)
    } else {
        0
    }
}
