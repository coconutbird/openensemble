use glam::Vec3;
use sim::entities::squads::SquadJumpPhase;
use sim::{
    CommandEntry, CommandExecutor, GameplayCatalog, QueuedCommand, RecoveryType, WorkCommand,
    World, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id,
};

const BRUTE: &str = "cov_inf_brute_01";
const UPGRADE_1: &str = "cov_brute_upgrade1";
const UPGRADE_2: &str = "cov_brute_upgrade2";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-jump -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_brute_uses_layered_jump_tactics_techs_and_ability() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    assert_shipped_jump_profiles(gameplay);

    let world = &mut loaded.simulation.world;
    let origin = scenario_center(world);
    let mut target = origin + Vec3::X * 20.0;
    target.y = world.terrain_height(target, true).unwrap_or(origin.y);
    let squad_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, BRUTE).expect("installed Brute squad"),
        origin,
        Vec3::X,
    )
    .expect("Brute squad should spawn");
    let unit_ids = world.get_squad(squad_id).unwrap().unit_ids.clone();
    assert_eq!(unit_ids.len(), 2);
    assert!(world.get_player(1).unwrap().technologies.is_active("basic"));

    execute_jump(world, database, gameplay, squad_id, target);
    assert_eq!(
        world.get_squad(squad_id).unwrap().jump_phase(),
        SquadJumpPhase::Inactive,
        "the shipped basic Shadow tech should gate CovJumppack",
    );

    world
        .activate_technology(1, database, UPGRADE_1)
        .expect("Brute tier-one technology should resolve");
    assert!(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .is_active(UPGRADE_1)
    );
    assert_eq!(world.activate_technology(1, database, UPGRADE_2), Ok(true));
    let checksum_before = world.checksum();
    execute_jump(world, database, gameplay, squad_id, target);
    assert_eq!(
        world.get_squad(squad_id).unwrap().jump_phase(),
        SquadJumpPhase::Pending,
    );
    assert_ne!(world.checksum(), checksum_before);
    assert!(unit_ids.iter().all(|unit_id| {
        world.get_unit(*unit_id).is_some_and(|unit| {
            unit.is_jumping() && unit.has_active_move_action() && !unit.is_attackable()
        })
    }));

    tick(world, database, gameplay);
    assert_eq!(
        world.get_squad(squad_id).unwrap().jump_phase(),
        SquadJumpPhase::Flying,
    );
    let mut observed_arc = false;
    for _ in 0..700 {
        tick(world, database, gameplay);
        observed_arc |= unit_ids.iter().any(|unit_id| {
            world
                .get_unit(*unit_id)
                .is_some_and(|unit| unit.base.position.y > origin.y + 1.0)
        });
        if !world.get_squad(squad_id).unwrap().is_jumping() {
            break;
        }
    }

    assert!(observed_arc, "the shipped spline should rise above terrain");
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.jump_phase(), SquadJumpPhase::Inactive);
    assert!(squad.base.position.distance(target) < 5.0);
    assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
    let jump_ability = gameplay
        .resolve_order_ability(BRUTE, gameplay.command_ability_id().unwrap())
        .unwrap();
    assert_eq!(
        squad.recovery.ability_id(),
        Some(jump_ability.database_id())
    );
    assert!(squad.recovery.remaining() > 0.0);
    assert!(squad.recovery.remaining() <= 15.0);
    assert!(unit_ids.iter().all(|unit_id| {
        world.get_unit(*unit_id).is_some_and(|unit| {
            !unit.is_jumping() && !unit.has_active_move_action() && unit.is_attackable()
        })
    }));
}

fn assert_shipped_jump_profiles(gameplay: &GameplayCatalog) {
    let profiles = gameplay.jump_actions(BRUTE);
    assert_eq!(profiles.len(), 4);
    assert_eq!(
        profiles
            .iter()
            .map(sim::gameplay::JumpActionProfile::action_name)
            .collect::<Vec<_>>(),
        [
            "JumpPack",
            "JumpPackGather",
            "JumpPackGarrison",
            "JumpAttackAction",
        ],
    );
    for profile in profiles {
        assert_eq!(profile.max_distance().to_bits(), 175.0_f32.to_bits());
        assert_eq!(profile.velocity_scalar().to_bits(), 40.0_f32.to_bits());
        assert_eq!(profile.weapon_name(), Some("Brutegun"));
        assert_eq!(profile.weapon_max_range().to_bits(), 35.0_f32.to_bits());
        assert!(profile.starts_disabled());
        assert!(!profile.ability_starts_disabled());
    }
}

fn execute_jump(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    squad_id: sim::EntityId,
    target: Vec3,
) {
    let command =
        WorkCommand::jump_squads(1, vec![squad_id], target, gameplay.command_ability_id());
    CommandExecutor::with_database_and_gameplay(database, gameplay).execute(
        world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
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
