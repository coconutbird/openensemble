//! Checks that the shipped Cobra uses the scenario-layered mode database and animations.

use glam::Vec3;
use sim::{
    CommandEntry, CommandExecutor, CommandPayload, EntityId, GameplayCatalog, SquadChangeModePhase,
    SquadMode, WorkCommand, World, load_scenario_from_game_dir, object_prototype_id,
    spawn_object_at, spawn_squad_at, squad_prototype_id,
};

const COBRA: &str = "unsc_veh_cobra_01";
const ELEPHANT: &str = "unsc_veh_elephant_01";
const MARINE: &str = "unsc_inf_marine_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-change-mode -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_cobra_uses_scenario_layered_mode_database_and_animations() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    assert_shipped_database(database, gameplay);

    let world = &mut loaded.simulation.world;
    let origin = scenario_center(world);
    let squad_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, COBRA).expect("installed Cobra squad"),
        origin,
        Vec3::X,
    )
    .expect("Cobra squad should spawn");
    let unit_id = *world
        .get_squad(squad_id)
        .and_then(|squad| squad.unit_ids.first())
        .expect("Cobra squad member");

    execute_toggle(world, database, gameplay, squad_id);
    assert_eq!(
        world.get_squad(squad_id).unwrap().change_mode_phase(),
        SquadChangeModePhase::Pending
    );
    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Lockdown)
    );
    assert!(!world.issue_move_order(1, squad_id, origin + Vec3::X * 20.0));
    assert!(advance_mode(
        world,
        database,
        gameplay,
        squad_id,
        unit_id,
        "AnchorDown"
    ));
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.mode, SquadMode::Lockdown);
    assert!(!squad.can_accept_movement_order());

    execute_toggle(world, database, gameplay, squad_id);
    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Normal)
    );
    assert!(advance_mode(
        world, database, gameplay, squad_id, unit_id, "AnchorUp"
    ));
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.mode, SquadMode::Normal);
    assert!(squad.can_accept_movement_order());

    let first_target = terrain_target(world, origin + Vec3::X * 20.0);
    execute_ability_move(world, database, gameplay, squad_id, first_target);
    assert_eq!(world.get_squad(squad_id).unwrap().mode, SquadMode::Normal);
    assert_eq!(
        world.get_squad(squad_id).unwrap().move_target,
        Some(first_target)
    );
    let (saw_unlock, saw_relock) =
        advance_ability_move(world, database, gameplay, squad_id, unit_id, first_target);
    assert!(!saw_unlock);
    assert!(saw_relock);

    let second_target = terrain_target(world, first_target + Vec3::X * 20.0);
    execute_ability_move(world, database, gameplay, squad_id, second_target);
    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Normal)
    );
    let (saw_unlock, saw_relock) =
        advance_ability_move(world, database, gameplay, squad_id, unit_id, second_target);
    assert!(saw_unlock);
    assert!(saw_relock);

    assert_shipped_cobra_minimum_range(world, database, gameplay, squad_id, unit_id);

    assert_shipped_elephant_lockdown_menu(world, database, gameplay, origin + Vec3::Z * 100.0);
}

fn assert_shipped_database(
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    let cobra_gameplay = gameplay.object(COBRA).expect("installed Cobra gameplay");
    let mobile_attack = cobra_gameplay
        .attack_profile("ArtilleryAttackAction")
        .expect("installed mobile Cobra attack");
    assert_eq!(mobile_attack.min_range.to_bits(), 0.0_f32.to_bits());
    assert_eq!(mobile_attack.max_range.to_bits(), 60.0_f32.to_bits());
    let lockdown_attack = cobra_gameplay
        .attack_profile("LockdownAttackAction")
        .expect("installed lockdown Cobra attack");
    assert_eq!(lockdown_attack.min_range.to_bits(), 6.0_f32.to_bits());
    assert_eq!(lockdown_attack.max_range.to_bits(), 120.0_f32.to_bits());
    let cobra = database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(COBRA))
        .expect("scenario-layered Cobra object");
    assert_eq!(cobra.ability_command.as_deref(), Some("UnscCobraLockdown"));
    assert!(
        cobra
            .squad_mode_animations
            .iter()
            .any(|animation| animation.animation.eq_ignore_ascii_case("AnchorDown"))
    );
    assert!(
        cobra
            .squad_mode_animations
            .iter()
            .any(|animation| animation.animation.eq_ignore_ascii_case("AnchorUp"))
    );

    let command_id = gameplay.command_ability_id().expect("Command ability");
    let ability = gameplay
        .resolve_order_ability(COBRA, command_id)
        .expect("Cobra command ability");
    assert_eq!(ability.name(), "UnscCobraLockdown");
    assert_eq!(ability.ability_type(), Some("ChangeMode"));
    assert_eq!(ability.squad_mode(), Some(SquadMode::Lockdown));
    assert_eq!(
        gameplay.squad_mode_animation(COBRA, SquadMode::Lockdown),
        Some("AnchorDown")
    );
    assert_eq!(
        gameplay.squad_mode_animation(COBRA, SquadMode::Normal),
        Some("AnchorUp")
    );

    let elephant = database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(ELEPHANT))
        .expect("scenario-layered Elephant object");
    assert_eq!(elephant.ability_command.as_deref(), Some("UnscLockdown"));
    assert_eq!(elephant.goto_type.as_deref(), Some("Vehicle"));
    assert_eq!(elephant.rally_point.as_deref(), Some("Military"));
    assert!(
        elephant
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("LockdownMenu"))
    );
}

fn assert_shipped_cobra_minimum_range(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    cobra_squad_id: EntityId,
    cobra_unit_id: EntityId,
) {
    let source_position = world.get_squad(cobra_squad_id).unwrap().base.position;
    let source_radius = squad_radius(world, cobra_squad_id);
    let close_position = terrain_target(world, source_position + Vec3::X * (source_radius + 5.0));
    let target_id = spawn_object_at(
        world,
        database,
        2,
        object_prototype_id(database, MARINE).expect("installed Marine object"),
        close_position,
        -Vec3::X,
    )
    .expect("standalone Marine target should spawn");
    let target = world.get_unit_mut(target_id).unwrap();
    target.max_hitpoints = 1_000_000.0;
    target.hitpoints = target.max_hitpoints;

    execute_ability_attack(world, database, gameplay, cobra_squad_id, target_id);
    assert_eq!(
        world.get_squad(cobra_squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Normal),
        "a commanded close attack must unlock deployed Cobra artillery"
    );

    let mut saw_unlock = false;
    for _ in 0..200 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        saw_unlock |= world
            .entity_scripted_animation(cobra_unit_id)
            .is_some_and(|animation| animation.animation_type() == "AnchorUp");
        let squad = world.get_squad(cobra_squad_id).unwrap();
        if squad.mode == SquadMode::Normal && !squad.is_changing_mode() {
            break;
        }
    }
    assert!(saw_unlock);
    assert_eq!(
        world.get_squad(cobra_squad_id).unwrap().mode,
        SquadMode::Normal
    );

    let close_hitpoints = world.get_unit(target_id).unwrap().hitpoints;
    for _ in 0..40 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        let action = world
            .get_unit(cobra_unit_id)
            .and_then(|unit| unit.combat.action_name());
        assert_ne!(action, Some("LockdownAttackAction"));
        let squad = world.get_squad(cobra_squad_id).unwrap();
        assert_eq!(squad.mode, SquadMode::Normal);
        assert!(!squad.is_changing_mode());
    }
    assert_eq!(
        world.get_unit(target_id).unwrap().hitpoints.to_bits(),
        close_hitpoints.to_bits()
    );

    let far_position = terrain_target(world, source_position + Vec3::X * (source_radius + 12.0));
    let target = world.get_unit_mut(target_id).unwrap();
    target.base.position = far_position;
    target.base.velocity = Vec3::ZERO;
    let hitpoints_before_lockdown = target.hitpoints;
    let mut saw_relock = false;
    let mut used_lockdown_attack = false;
    let mut dealt_lockdown_damage = false;
    for _ in 0..600 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        saw_relock |= world
            .entity_scripted_animation(cobra_unit_id)
            .is_some_and(|animation| animation.animation_type() == "AnchorDown");
        used_lockdown_attack |= world
            .get_unit(cobra_unit_id)
            .and_then(|unit| unit.combat.action_name())
            == Some("LockdownAttackAction");
        dealt_lockdown_damage |=
            world.get_unit(target_id).unwrap().hitpoints < hitpoints_before_lockdown;
        let squad = world.get_squad(cobra_squad_id).unwrap();
        if saw_relock
            && used_lockdown_attack
            && dealt_lockdown_damage
            && squad.mode == SquadMode::Lockdown
            && !squad.is_changing_mode()
        {
            break;
        }
    }
    assert!(saw_relock);
    assert!(used_lockdown_attack);
    assert!(dealt_lockdown_damage);
    assert_eq!(
        world.get_squad(cobra_squad_id).unwrap().mode,
        SquadMode::Lockdown
    );
}

fn squad_radius(world: &World, squad_id: EntityId) -> f32 {
    world
        .get_squad(squad_id)
        .unwrap()
        .unit_ids
        .iter()
        .filter_map(|unit_id| world.get_unit(*unit_id))
        .map(|unit| {
            let offset = Vec3::new(unit.formation_offset.x, 0.0, unit.formation_offset.z);
            let obstruction = unit
                .obstruction_half_extents
                .x
                .abs()
                .max(unit.obstruction_half_extents.z.abs());
            offset.length() + obstruction
        })
        .fold(0.0, f32::max)
}

fn assert_shipped_elephant_lockdown_menu(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    position: Vec3,
) {
    let position = terrain_target(world, position);
    let squad_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, ELEPHANT).expect("installed Elephant squad"),
        position,
        Vec3::X,
    )
    .expect("Elephant squad should spawn");
    let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];

    execute_toggle(world, database, gameplay, squad_id);
    assert!(advance_mode(
        world,
        database,
        gameplay,
        squad_id,
        unit_id,
        "AnchorDown"
    ));
    assert!(
        world
            .get_player(1)
            .unwrap()
            .goto_base_units()
            .contains(&unit_id)
    );
    let unit_position = world.get_unit(unit_id).unwrap().base.position;
    let rally = world
        .unit_rally_point(unit_id, 1)
        .expect("deployed Elephant default rally")
        .position();
    let planar_offset = Vec3::new(rally.x - unit_position.x, 0.0, rally.z - unit_position.z);
    assert!((planar_offset.length() - 50.0).abs() < 0.01);

    execute_toggle(world, database, gameplay, squad_id);
    assert!(advance_mode(
        world, database, gameplay, squad_id, unit_id, "AnchorUp"
    ));
    assert!(
        !world
            .get_player(1)
            .unwrap()
            .goto_base_units()
            .contains(&unit_id)
    );
    assert!(world.unit_rally_point(unit_id, 1).is_none());
}

fn execute_ability_attack(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
    target_id: EntityId,
) {
    let mut command = WorkCommand::attack_squads(1, vec![squad_id], target_id);
    command.ability_id = gameplay.command_ability_id().map_or(-1, i32::from);
    CommandExecutor::with_database_and_gameplay(database, gameplay).execute(
        world,
        &CommandEntry {
            command: CommandPayload::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn execute_toggle(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
) {
    let command =
        WorkCommand::change_mode_squads(1, vec![squad_id], None, gameplay.command_ability_id());
    CommandExecutor::with_database_and_gameplay(database, gameplay).execute(
        world,
        &CommandEntry {
            command: CommandPayload::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn execute_ability_move(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
    target: Vec3,
) {
    let mut command = WorkCommand::move_squads(1, vec![squad_id], target);
    command.ability_id = gameplay.command_ability_id().map_or(-1, i32::from);
    CommandExecutor::with_database_and_gameplay(database, gameplay).execute(
        world,
        &CommandEntry {
            command: CommandPayload::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn advance_ability_move(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
    unit_id: EntityId,
    target: Vec3,
) -> (bool, bool) {
    let mut saw_unlock = false;
    let mut saw_relock = false;
    for _ in 0..1_000 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        if let Some(animation) = world.entity_scripted_animation(unit_id) {
            saw_unlock |= animation.animation_type() == "AnchorUp";
            saw_relock |= animation.animation_type() == "AnchorDown";
        }
        let squad = world.get_squad(squad_id).unwrap();
        if squad.mode == SquadMode::Lockdown
            && !squad.is_changing_mode()
            && squad.base.position.distance(target) < 1.0
        {
            return (saw_unlock, saw_relock);
        }
    }
    panic!("Cobra ability movement did not move and relock");
}

fn advance_mode(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    squad_id: EntityId,
    unit_id: EntityId,
    expected_animation: &str,
) -> bool {
    let mut observed_animation = false;
    for _ in 0..200 {
        world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
        observed_animation |= world
            .entity_scripted_animation(unit_id)
            .is_some_and(|animation| animation.animation_type() == expected_animation);
        if !world.get_squad(squad_id).unwrap().is_changing_mode() {
            return observed_animation;
        }
    }
    panic!("Cobra mode transition did not complete");
}

fn scenario_center(world: &World) -> Vec3 {
    let bounds = world.terrain_bounds().expect("scenario terrain bounds");
    let mut center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    center.y = world.terrain_height(center, true).unwrap_or_default();
    center
}

fn terrain_target(world: &World, mut target: Vec3) -> Vec3 {
    target.y = world.terrain_height(target, true).unwrap_or(target.y);
    target
}
