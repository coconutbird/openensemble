//! Damage over time stacking, the projectile daze flag, the stasis bomb and smart targeting.

use super::dot::add_stack;
use crate::entities::units::weapon_effects::{DotStack, StasisHold};
use crate::gameplay::{
    DazeProfile, GameplayCatalog, SmartTargetType, StasisProfile, WeaponEffectProfile,
};
use crate::world::World;
use crate::world::combat::test_catalog::{instant_profile, ranged_action};
use crate::world::combat::{ProjectileHit, WeaponEffectHit};
use glam::Vec3;
use pipeline::database::hw1::tactics::{Stasis, TacticData, TacticRules, TargetRule, Weapon};
use pipeline::database::hw1::{Ability, Database, ProtoObject};
use std::cell::Cell;
use std::sync::Arc;

const ATTACKER: &str = "test_attacker";
const TARGET: &str = "test_target";

fn two_player_world() -> World {
    let mut world = World::new();
    world.init_players(3);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.set_fog_of_war_enabled(false);
    world
}

fn spawn_squad(world: &mut World, player_id: u8, proto: &str, position: Vec3) -> crate::EntityId {
    let unit_id = world.create_unit_at(player_id, position);
    let unit = world.units.get_mut(unit_id).unwrap();
    unit.proto_object_name = proto.to_owned();
    unit.object_types = vec!["Infantry".to_owned()];
    unit.max_hitpoints = 100.0;
    unit.hitpoints = 100.0;
    let squad_id = world.create_squad_at(player_id, position);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    squad_id
}

fn leader(world: &World, squad_id: crate::EntityId) -> crate::EntityId {
    world.get_squad(squad_id).unwrap().unit_ids[0]
}

#[test]
fn dot_stacks_refresh_the_source_join_a_match_and_else_start_an_action() {
    let mut world = two_player_world();
    let first = world.create_unit_at(1, Vec3::ZERO);
    let second = world.create_unit_at(1, Vec3::ZERO);
    let third = world.create_unit_at(1, Vec3::ZERO);
    let stack = |source, end_time_ms| DotStack {
        source_id: Some(source),
        source_player_id: 1,
        source_team_id: 1,
        end_time_ms,
    };
    let mut actions = Vec::new();
    add_stack(&mut actions, stack(first, 1_000), Some("fx"), 6.8, 1.0);
    // The same source refreshes its stack.
    add_stack(&mut actions, stack(first, 2_000), Some("fx"), 6.8, 1.0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].stacks.len(), 1);
    assert_eq!(actions[0].stacks[0].end_time_ms, 2_000);
    // Another source with the same effect, rate and multiplier joins the action.
    add_stack(&mut actions, stack(second, 3_000), Some("fx"), 6.8, 1.0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].stacks.len(), 2);
    // Another multiplier starts a new action.
    add_stack(&mut actions, stack(third, 4_000), Some("fx"), 6.8, 0.5);
    assert_eq!(actions.len(), 2);
    assert!(!actions[1].working);
    // The same source of another team neither refreshes nor adds a stack.
    let mut other_team = stack(first, 5_000);
    other_team.source_team_id = 2;
    add_stack(&mut actions, other_team, Some("fx"), 6.8, 1.0);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].stacks.len(), 2);
    assert_eq!(actions[0].stacks[0].end_time_ms, 2_000);
}

fn daze_effects(aoe: bool) -> WeaponEffectProfile {
    WeaponEffectProfile {
        damage_over_time: None,
        daze: Some(DazeProfile {
            duration_ms: 3_000.0,
            target_type: Some("Infantry".to_owned()),
            movement_modifier: 0.0,
            aoe,
        }),
        stasis: StasisProfile::default(),
        smart_target: None,
        damage_per_second: 0.0,
    }
}

#[test]
fn a_projectile_without_aoe_daze_dazes_one_squad_only() {
    let mut world = two_player_world();
    let first = spawn_squad(&mut world, 2, TARGET, Vec3::ZERO);
    let second = spawn_squad(&mut world, 2, TARGET, Vec3::X * 2.0);
    let (first_unit, second_unit) = (leader(&world, first), leader(&world, second));
    let effects = daze_effects(false);
    let projectile = ProjectileHit {
        target_id: first_unit,
        target_already_dazed: Cell::new(false),
    };
    let hit = WeaponEffectHit {
        profile: &effects,
        projectile: Some(&projectile),
    };
    world.daze_on_hit(hit, None, first_unit, None);
    assert_eq!(
        world.get_squad(first).unwrap().daze_remaining_ms(),
        Some(3_000)
    );
    assert!(projectile.target_already_dazed.get());
    // The area of the same impact does not daze the second squad.
    world.daze_on_hit(hit, None, second_unit, None);
    assert!(!world.get_squad(second).unwrap().is_dazed());

    // With `AOEDaze`, the flag stays clear and the area dazes every squad.
    let effects = daze_effects(true);
    let projectile = ProjectileHit {
        target_id: first_unit,
        target_already_dazed: Cell::new(false),
    };
    let hit = WeaponEffectHit {
        profile: &effects,
        projectile: Some(&projectile),
    };
    world.daze_on_hit(hit, None, second_unit, None);
    assert!(world.get_squad(second).unwrap().is_dazed());
    assert!(!projectile.target_already_dazed.get());
}

fn stasis_bomb_catalog() -> GameplayCatalog {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: ATTACKER.to_owned(),
        tactics: Some("test_attacker.tactics".to_owned()),
        ..ProtoObject::default()
    });
    let beam = Weapon {
        name: "StasisBombBeam".to_owned(),
        max_range: Some(30.0),
        damage_per_second: Some(10.0),
        stasis_bomb: Some(true),
        ..Weapon::default()
    };
    let bomb = Weapon {
        name: "StasisBomb".to_owned(),
        damage_per_second: Some(40.0),
        aoe_radius: Some(10.0),
        ..Weapon::default()
    };
    let mut beam_action = ranged_action("StasisBombBeamAttackAction", "StasisBombBeam");
    beam_action.linked_action = Some("StasisBombAction".to_owned());
    let bomb_action = ranged_action("StasisBombAction", "StasisBomb");
    let mut profile = instant_profile("StasisBombBeamAttackAction", 30.0);
    profile.weapon_effects = WeaponEffectProfile::from_action(&beam_action, &beam).map(Arc::new);
    let tactics = TacticData {
        weapons: vec![beam, bomb],
        actions: vec![beam_action, bomb_action],
        ..TacticData::default()
    };
    GameplayCatalog::from_test_profiles(
        &database,
        [(ATTACKER.to_owned(), tactics)],
        [(ATTACKER.to_owned(), profile)],
    )
}

#[test]
fn a_stasis_bomb_hold_detonates_only_at_a_target_with_one_hitpoint_or_less() {
    let gameplay = stasis_bomb_catalog();
    let mut world = two_player_world();
    let attacker = spawn_squad(&mut world, 1, ATTACKER, Vec3::new(0.0, 0.0, -20.0));
    let target = spawn_squad(&mut world, 2, TARGET, Vec3::ZERO);
    let bystander = spawn_squad(&mut world, 2, TARGET, Vec3::X * 3.0);
    let (attacker_unit, target_unit) = (leader(&world, attacker), leader(&world, target));
    let bystander_unit = leader(&world, bystander);
    let hold = StasisHold {
        target_id: target_unit,
        action_name: "StasisBombBeamAttackAction".to_owned(),
        drain: false,
        bomb: true,
    };
    let start_hold = |world: &mut World| {
        world
            .units
            .get_mut(attacker_unit)
            .unwrap()
            .weapon_effects
            .stasis_hold = Some(hold.clone());
        world
            .squads
            .get_mut(target)
            .unwrap()
            .weapon_effects
            .stasis_effects = 1;
    };

    // Above 1 hit point, the hold releases without damage.
    start_hold(&mut world);
    world.update_stasis_holds(&gameplay);
    assert_eq!(
        world.get_unit(bystander_unit).unwrap().hitpoints.to_bits(),
        100.0_f32.to_bits()
    );
    assert!(
        world
            .get_unit(attacker_unit)
            .unwrap()
            .weapon_effects
            .stasis_hold
            .is_none()
    );
    assert_eq!(world.get_squad(target).unwrap().stasis_effect_count(), 0);

    // At 1 hit point, the linked area damage hits the units near the target.
    world.units.get_mut(target_unit).unwrap().hitpoints = 1.0;
    start_hold(&mut world);
    world.update_stasis_holds(&gameplay);
    assert!(world.get_unit(bystander_unit).unwrap().hitpoints < 100.0);
    assert_eq!(world.get_squad(target).unwrap().stasis_effect_count(), 0);
}

fn smart_target_catalog() -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.abilities.extend([
        Ability {
            name: "Command".to_owned(),
            ..Ability::default()
        },
        Ability {
            name: "TestStasis".to_owned(),
            smart_target_range: Some(30.0),
            ..Ability::default()
        },
    ]);
    database.objects.extend([
        ProtoObject {
            name: ATTACKER.to_owned(),
            tactics: Some("test_attacker.tactics".to_owned()),
            ability_command: Some("TestStasis".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: TARGET.to_owned(),
            object_types: vec!["Infantry".to_owned()],
            num_stasis_fields_to_stop: Some(1),
            ..ProtoObject::default()
        },
    ]);
    let weapon = Weapon {
        name: "StasisGun".to_owned(),
        max_range: Some(60.0),
        damage_per_second: Some(1.0),
        stasis: Some(Stasis {
            smart_targeting: Some(true),
        }),
        ..Weapon::default()
    };
    let action = ranged_action("StasisAttack", "StasisGun");
    let mut profile = instant_profile("StasisAttack", 60.0);
    profile.weapon_effects = WeaponEffectProfile::from_action(&action, &weapon).map(Arc::new);
    let tactics = TacticData {
        weapons: vec![weapon],
        actions: vec![action],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Enemy".to_owned()),
                squad_mode: Some("Normal".to_owned()),
                action: Some("StasisAttack".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_test_profiles(
        &database,
        [(ATTACKER.to_owned(), tactics)],
        [(ATTACKER.to_owned(), profile)],
    );
    (database, gameplay)
}

#[test]
fn smart_targeting_spreads_stasis_attacks_and_releases_at_the_attack_end() {
    let (database, gameplay) = smart_target_catalog();
    let mut world = two_player_world();
    world.configure_prototype_catalogs(&database);
    let first_target = spawn_squad(&mut world, 2, TARGET, Vec3::new(0.0, 0.0, 10.0));
    let second_target = spawn_squad(&mut world, 2, TARGET, Vec3::new(4.0, 0.0, 10.0));
    let attackers = [-4.0_f32, 0.0, 4.0].map(|x| spawn_squad(&mut world, 1, ATTACKER, Vec3::X * x));
    let references = |world: &World, squad_id| {
        world
            .get_squad(squad_id)
            .unwrap()
            .smart_target_references(SmartTargetType::Stasis)
    };
    let attack = |world: &mut World, squad_id| {
        assert!(
            world
                .squads
                .get_mut(squad_id)
                .unwrap()
                .attack(first_target, 0.0, None, Some(1))
        );
        world.update_smart_targets(&gameplay);
        world.get_squad(squad_id).unwrap().attack_target
    };

    // The first attack claims the ordered target.
    assert_eq!(attack(&mut world, attackers[0]), Some(first_target));
    assert_eq!(references(&world, first_target), 1);
    // The ordered target is full, so the second attack moves to the free squad nearby.
    assert_eq!(attack(&mut world, attackers[1]), Some(second_target));
    assert_eq!(references(&world, second_target), 1);
    assert_eq!(
        world.get_squad(attackers[1]).unwrap().attack_ability_id,
        Some(1)
    );
    // Every candidate is full: the last candidate is the target and the ability goes.
    assert_eq!(attack(&mut world, attackers[2]), Some(second_target));
    assert_eq!(references(&world, second_target), 1);
    assert_eq!(
        world.get_squad(attackers[2]).unwrap().attack_ability_id,
        None
    );

    // The end of an attack releases its reference.
    world
        .squads
        .get_mut(attackers[0])
        .unwrap()
        .clear_attack_order();
    world.update_smart_targets(&gameplay);
    assert_eq!(references(&world, first_target), 0);
    assert_eq!(
        world
            .get_squad(attackers[0])
            .unwrap()
            .weapon_effects
            .smart_target_choice,
        None
    );
}

#[test]
fn an_attack_blocked_squad_finds_no_attack_move_target() {
    let (database, gameplay) = smart_target_catalog();
    let mut world = two_player_world();
    world.configure_prototype_catalogs(&database);
    let target = spawn_squad(&mut world, 2, TARGET, Vec3::new(0.0, 0.0, 10.0));
    let attacker = spawn_squad(&mut world, 1, ATTACKER, Vec3::ZERO);
    {
        let squad = world.squads.get_mut(attacker).unwrap();
        squad.aggro_distance = 50.0;
        assert!(squad.issue_scripted_move(Vec3::new(0.0, 0.0, 100.0), true, false));
        squad.weapon_effects.attack_blocked = true;
    }
    world.update_attack_move_orders(&gameplay);
    assert_eq!(world.get_squad(attacker).unwrap().attack_target, None);

    world
        .squads
        .get_mut(attacker)
        .unwrap()
        .weapon_effects
        .attack_blocked = false;
    world.update_attack_move_orders(&gameplay);
    let squad = world.get_squad(attacker).unwrap();
    assert!(
        squad
            .attack_target
            .is_some_and(|id| id == target || id == leader(&world, target))
    );
}
