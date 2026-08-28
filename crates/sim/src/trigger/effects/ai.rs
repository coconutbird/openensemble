//! Retail AI squad-analysis and force-comparison effects.

use super::support::{player_at, used_variable_id};
use super::{EffectOutcome, value_at, write_value};
use crate::entities::SquadMode;
use crate::gameplay::GameplayCatalog;
use crate::player::Player;
use crate::trigger::ai_analysis::{AI_DAMAGE_CLASS_COUNT, AIDamageClass};
use crate::trigger::{AISquadAnalysis, Effect, TriggerScript, TriggerValue};
use crate::world::World;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug, Clone, Copy)]
struct PrototypeMetrics {
    damage_class: Option<AIDamageClass>,
    combat_value: f32,
    maximum_hitpoints: f32,
    maximum_shieldpoints: f32,
    base_damage_per_second: f32,
    attack_ratings: [f32; AI_DAMAGE_CLASS_COUNT],
}

impl Default for PrototypeMetrics {
    fn default() -> Self {
        Self {
            damage_class: None,
            combat_value: 0.0,
            maximum_hitpoints: 0.0,
            maximum_shieldpoints: 0.0,
            base_damage_per_second: 0.0,
            attack_ratings: [0.0; AI_DAMAGE_CLASS_COUNT],
        }
    }
}

pub(super) fn analyze_squad_list(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    database: Option<&Database>,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(TriggerValue::SquadList(squad_ids)) = value_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let mut analysis = AISquadAnalysis::default();
    for squad_id in squad_ids {
        add_live_squad(&mut analysis, *squad_id, world, database, gameplay);
    }
    analysis.finish();
    write_value(script, output_id, TriggerValue::AISquadAnalysis(analysis))
}

pub(super) fn analyze_proto_squad_list(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    database: Option<&Database>,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(TriggerValue::ProtoSquadList(proto_ids)) = value_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(player_id) = player_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let mut analysis = AISquadAnalysis::default();
    if let Some(player) = world.get_player(player_id) {
        for proto_id in proto_ids {
            let Some(logical) = squad_by_id(database, *proto_id) else {
                continue;
            };
            let metrics = prototype_metrics(database, player, logical, gameplay);
            analysis.add_squad(
                metrics.damage_class,
                metrics.combat_value,
                metrics.maximum_hitpoints,
                metrics.maximum_shieldpoints,
                metrics.base_damage_per_second,
                metrics.attack_ratings,
            );
        }
    }
    analysis.finish();
    write_value(script, output_id, TriggerValue::AISquadAnalysis(analysis))
}

pub(super) fn get_component(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(TriggerValue::AISquadAnalysis(analysis)) = value_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(TriggerValue::AISquadAnalysisComponent(component)) = value_at(effect, script, 2)
    else {
        return EffectOutcome::Skipped;
    };
    let value = analysis.component(*component);
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Float(value))
}

pub(super) fn analyze_offense(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    compare_analyses(effect, script, AISquadAnalysis::offense_against)
}

pub(super) fn calculate_offense_ratio(
    effect: &Effect,
    script: &mut TriggerScript,
) -> EffectOutcome {
    compare_analyses(effect, script, AISquadAnalysis::offense_ratio_against)
}

fn compare_analyses(
    effect: &Effect,
    script: &mut TriggerScript,
    operation: impl FnOnce(&AISquadAnalysis, &AISquadAnalysis) -> f32,
) -> EffectOutcome {
    let value = {
        let Some(TriggerValue::AISquadAnalysis(first)) = value_at(effect, script, 1) else {
            return EffectOutcome::Skipped;
        };
        let Some(TriggerValue::AISquadAnalysis(second)) = value_at(effect, script, 2) else {
            return EffectOutcome::Skipped;
        };
        operation(first, second)
    };
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    write_value(script, output_id, TriggerValue::Float(value))
}

fn add_live_squad(
    analysis: &mut AISquadAnalysis,
    squad_id: crate::EntityId,
    world: &World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    let Some(player) = world.get_player(squad.base.player_id) else {
        return;
    };
    let Some(logical) = squad_by_id(database, squad.proto_squad_id).or_else(|| {
        database
            .squads
            .iter()
            .find(|proto| proto.name.eq_ignore_ascii_case(&squad.proto_squad_name))
    }) else {
        return;
    };
    let metrics = prototype_metrics(database, player, logical, gameplay);
    let (hitpoints, shieldpoints) = squad.unit_ids.iter().fold((0.0, 0.0), |total, unit_id| {
        world.get_unit(*unit_id).map_or(total, |unit| {
            (total.0 + unit.hitpoints, total.1 + unit.shields.current)
        })
    });
    let maximum_health = metrics.maximum_hitpoints + metrics.maximum_shieldpoints;
    let health_fraction = if maximum_health > 0.0 {
        (hitpoints + shieldpoints) / maximum_health
    } else {
        0.0
    };
    let cover_multiplier = if squad.mode == SquadMode::Cover {
        3.0
    } else {
        1.0
    };
    analysis.add_squad(
        metrics.damage_class,
        metrics.combat_value * health_fraction * cover_multiplier,
        hitpoints,
        shieldpoints,
        metrics.base_damage_per_second,
        metrics.attack_ratings,
    );
}

fn prototype_metrics(
    database: &Database,
    player: &Player,
    logical: &ProtoSquad,
    gameplay: Option<&GameplayCatalog>,
) -> PrototypeMetrics {
    let resolved_name = player.technologies.resolved_squad_prototype(&logical.name);
    let prototype = database
        .squads
        .iter()
        .find(|proto| proto.name.eq_ignore_ascii_case(resolved_name))
        .unwrap_or(logical);
    let mut metrics = PrototypeMetrics::default();
    let mut found_first_object = false;
    for member in prototype
        .units
        .as_ref()
        .map_or(&[][..], |units| units.entries.as_slice())
    {
        if member.count <= 0 {
            continue;
        }
        let Some(object) = object_by_name(database, &member.proto_object) else {
            continue;
        };
        if !found_first_object {
            metrics.damage_class = object
                .damage_type
                .as_deref()
                .and_then(AIDamageClass::from_name);
            found_first_object = true;
        }
        add_member_metrics(&mut metrics, object, member.count, player, gameplay);
    }
    metrics
}

fn add_member_metrics(
    metrics: &mut PrototypeMetrics,
    object: &ProtoObject,
    count: i32,
    player: &Player,
    gameplay: Option<&GameplayCatalog>,
) {
    let Ok(count) = u16::try_from(count) else {
        return;
    };
    let count = f32::from(count);
    metrics.combat_value += finite_value(object.combat_value) * count;
    let hitpoints = finite_value(object.hitpoints);
    let shieldpoints = finite_value(object.shieldpoints);
    metrics.maximum_hitpoints += player.technologies.hitpoints(&object.name, hitpoints) * count;
    metrics.maximum_shieldpoints +=
        player.technologies.shieldpoints(&object.name, shieldpoints) * count;
    metrics.base_damage_per_second += object
        .attack_grade_dps
        .as_deref()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or_default()
        * count;
    if let Some(gameplay) = gameplay {
        let ratings = gameplay.ai_attack_ratings(&object.name, &player.technologies);
        for (total, rating) in metrics.attack_ratings.iter_mut().zip(ratings) {
            *total += rating * count;
        }
    }
}

fn squad_by_id(database: &Database, id: i32) -> Option<&ProtoSquad> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(index, proto)| database_id(proto.dbid, *index) == id)
        .map(|(_, proto)| proto)
}

fn object_by_name<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(name.trim()))
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn finite_value(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
