use glam::Vec3;
use sim::{
    GameplayCatalog, SquadPullPhase, World, load_scenario_from_game_dir, spawn_squad_at,
    squad_prototype_id,
};

const BRUTE_CHIEF_01: &str = "cov_inf_bruteChief_01";
const BRUTE_CHIEF_02: &str = "cov_inf_bruteChief_02";
const BRUTE_CHIEF_03: &str = "cov_inf_bruteChief_03";
const MARINES: &str = "unsc_inf_marine_01";
const UPGRADE_1: &str = "cov_bruteChief_upgrade1";
const UPGRADE_2: &str = "cov_bruteChief_upgrade2";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-charge -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_brute_chief_charge_compiles_and_pulls_a_marine_squad() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profiles(&loaded.simulation.gameplay);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let origin = scenario_center(world);
    let attacker_squad = spawn(world, database, 1, BRUTE_CHIEF_01, origin, Vec3::X);
    let target_squad = spawn(
        world,
        database,
        2,
        MARINES,
        origin + Vec3::X * 30.0,
        Vec3::NEG_X,
    );
    let attacker_id = world.get_squad(attacker_squad).unwrap().unit_ids[0];
    let target_hitpoints = squad_hitpoints(world, target_squad);

    assert_eq!(world.activate_technology(1, database, UPGRADE_1), Ok(true));
    assert_eq!(world.activate_technology(1, database, UPGRADE_2), Ok(true));
    assert_eq!(
        world.get_unit(attacker_id).unwrap().proto_object_name,
        BRUTE_CHIEF_03
    );
    world.update_entities_with_database_and_gameplay(10.0, database, gameplay);
    let attacker = world.get_unit(attacker_id).unwrap();
    assert!(attacker.charge_seconds() >= 10.0);
    assert!(attacker.charge_effect_entity_id().is_some());
    assert!(world.issue_attack_order(1, attacker_squad, target_squad, 0.0));

    for _ in 0..20 {
        tick(world, database, gameplay);
        if world
            .get_squad(target_squad)
            .is_some_and(|squad| squad.pull_phase() != SquadPullPhase::Inactive)
        {
            break;
        }
    }
    assert_ne!(
        world.get_squad(target_squad).unwrap().pull_phase(),
        SquadPullPhase::Inactive
    );
    assert!(
        world
            .get_squad(target_squad)
            .unwrap()
            .unit_ids
            .iter()
            .all(|unit_id| !world.get_unit(*unit_id).unwrap().is_attackable())
    );
    assert_close(squad_hitpoints(world, target_squad), target_hitpoints);

    for _ in 0..40 {
        tick(world, database, gameplay);
        if world
            .get_squad(target_squad)
            .is_some_and(|squad| squad.pull_phase() == SquadPullPhase::Inactive)
        {
            break;
        }
    }
    assert_eq!(
        world.get_squad(target_squad).unwrap().pull_phase(),
        SquadPullPhase::Inactive
    );
    assert!(
        world
            .get_squad(target_squad)
            .unwrap()
            .unit_ids
            .iter()
            .all(|unit_id| world.get_unit(*unit_id).unwrap().is_attackable())
    );
    assert_close(squad_hitpoints(world, target_squad), target_hitpoints);
    assert!(world.get_squad(target_squad).unwrap().base.position.x < origin.x + 10.0);
    assert!(
        world
            .get_unit(attacker_id)
            .unwrap()
            .charge_effect_entity_id()
            .is_none()
    );
}

fn assert_shipped_profiles(gameplay: &GameplayCatalog) {
    for prototype in [BRUTE_CHIEF_01, BRUTE_CHIEF_02, BRUTE_CHIEF_03] {
        let charge = gameplay.charge(prototype).expect("compiled Charge action");
        assert_eq!(charge.action_name(), "Charge");
        assert!(charge.starts_disabled());
        assert_close(charge.damage_charge(), 10.0);
        assert_eq!(charge.animation_type(), Some("Pull"));
        assert!(charge.charge_on_taken());
        assert!(charge.charge_on_dealt());
        let effect = charge.effect().expect("charged ready effect");
        assert_eq!(effect.prototype_name(), "fx_brutePullCharged");
        assert_eq!(effect.bone_name(), Some("BoneFX"));
        assert!(effect.prototype_id().is_some());

        let object = gameplay.object(prototype).expect("Brute Chief tactics");
        let pull = object
            .attack_profile("StunPullHammerAttackAction")
            .expect("single-target PullUnits attack profile");
        let charged_animation = pull
            .charged_animation
            .as_ref()
            .expect("authored Charge animation timeline");
        assert_eq!(charged_animation.animation_type, "Pull");
        assert!(charged_animation.animations.iter().any(|animation| {
            !animation.asset_path.is_empty() && !animation.attack_positions.is_empty()
        }));
        let pull = pull.pull.as_ref().expect("single-target PullUnits profile");
        assert_close(pull.max_range, 55.0);
        assert_close(pull.velocity_scalar, 40.0);
        assert_eq!(pull.end_animation_type.as_deref(), Some("Flail"));
        assert_eq!(pull.invalid_targets, ["cov_veh_scarab_01"]);

        let area_pull = object
            .attack_profile("AreaStunHammerAttackAction")
            .and_then(|profile| profile.pull.as_ref())
            .expect("area PullUnits profile");
        assert_close(area_pull.max_range, 55.0);
        assert_close(area_pull.velocity_scalar, 40.0);
    }
}

fn spawn(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    player_id: u8,
    prototype: &str,
    position: Vec3,
    forward: Vec3,
) -> sim::EntityId {
    spawn_squad_at(
        world,
        database,
        player_id,
        squad_prototype_id(database, prototype).expect("shipped squad prototype"),
        position,
        forward,
    )
    .expect("shipped squad should spawn")
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

fn squad_hitpoints(world: &World, squad_id: sim::EntityId) -> f32 {
    world.get_squad(squad_id).map_or(0.0, |squad| {
        squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .map(|unit| unit.hitpoints)
            .sum()
    })
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "{actual} != {expected}"
    );
}
