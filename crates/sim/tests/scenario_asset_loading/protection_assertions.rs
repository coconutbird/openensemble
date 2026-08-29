use super::nearly_equal;
use sim::{
    AttackQuery, JoinKind, JoinMergeType, LoadedGameScenario, TacticRelation, object_prototype_id,
    spawn_object_at, spawn_squad_at, squad_prototype_id,
};

pub(super) fn assert_real_protection_catalog_and_lifecycle(loaded: &mut LoadedGameScenario) {
    assert_real_protection_catalog(loaded);
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("the scenario should assign a player base")
        .0;
    assert_real_plasma_shield_lifecycle(loaded, player_id);
    assert_real_bubble_shield_lifecycle(loaded, player_id);
    assert_real_follow_attack_lifecycle(loaded, player_id);
    assert_real_merge_lifecycle(loaded, player_id);
    assert_real_board_lifecycle(loaded, player_id);
}

fn assert_real_protection_catalog(loaded: &LoadedGameScenario) {
    let gameplay = &loaded.simulation.gameplay;
    let profile = gameplay
        .plasma_shield_generator("COV_BLDG_SHIELDGEN_01")
        .expect("layered ShieldGen tactics should retain PlasmaShieldGen");
    assert_eq!(profile.shield_proto_object_name(), "cov_bldg_shield_01");
    assert!(nearly_equal(profile.rebuild_time(), 30.0));
    assert!(nearly_equal(profile.under_attack_wait(), 5.0));
    assert!(nearly_equal(profile.deflect_timeout(), 30.0));
    assert_eq!(profile.recharge_text_id(), Some(25_136));
    assert_eq!(
        gameplay.default_shield_bubble_squad(),
        Some("sys_bubbleshield_small_01")
    );
    assert_eq!(
        gameplay.shield_bubble_squad("unsc_veh_scorpion_01"),
        Some("sys_bubbleshield_med_01")
    );
    assert_eq!(
        gameplay.shield_bubble_squad("cov_veh_scarab_01"),
        Some("sys_bubbleshield_large_01")
    );
    let bubble_action = gameplay
        .bubble_shield_action("for_air_monitor_04")
        .expect("Protector monitor tactics should retain Follow/BubbleShield");
    assert_eq!(bubble_action.join_action_name(), "Join");
    assert!(nearly_equal(bubble_action.work_range(), 5.0));
    assert_eq!(bubble_action.merge_type(), Some("Air"));
    assert_real_join_catalog(loaded);
}

fn assert_real_join_catalog(loaded: &LoadedGameScenario) {
    let gameplay = &loaded.simulation.gameplay;
    let follow_attack = gameplay
        .select_join_action(
            "for_air_monitor_01",
            &AttackQuery {
                relation: TacticRelation::SelfPlayer,
                target_proto_object_name: Some("unsc_veh_scorpion_01"),
                ..AttackQuery::default()
            },
            |_| true,
        )
        .expect("scenario-layered monitor tactics should select FollowAttack");
    assert_eq!(follow_attack.kind(), JoinKind::FollowAttack);
    assert_eq!(follow_attack.merge_type(), JoinMergeType::Air);
    assert!(nearly_equal(follow_attack.work_range(), 0.5));
    let infantry_join = gameplay
        .select_join_action_for_squads(
            "unsc_inf_spartan_01",
            Some("unsc_inf_spartan_01"),
            Some("unsc_inf_marine_01"),
            &AttackQuery {
                relation: TacticRelation::SelfPlayer,
                ability_id: gameplay.command_ability_id(),
                target_proto_object_name: Some("unsc_inf_marine_01"),
                ..AttackQuery::default()
            },
            |_| true,
        )
        .expect("scenario-layered Spartan tactics should select InfantryJoin");
    assert_eq!(infantry_join.kind(), JoinKind::Merge);
    assert_eq!(infantry_join.merge_type(), JoinMergeType::Ground);
    assert!(nearly_equal(infantry_join.work_range(), 5.0));
    let vehicle_takeover = gameplay
        .select_join_action(
            "unsc_inf_spartan_01",
            &AttackQuery {
                relation: TacticRelation::Enemy,
                ability_id: gameplay.command_ability_id(),
                target_proto_object_name: Some("unsc_veh_scorpion_01"),
                ..AttackQuery::default()
            },
            |_| true,
        )
        .expect("scenario-layered Spartan tactics should select VehicleTakeOver");
    assert_eq!(vehicle_takeover.kind(), JoinKind::Board);
    assert_eq!(vehicle_takeover.merge_type(), JoinMergeType::Ground);
    assert!(nearly_equal(vehicle_takeover.work_range(), 8.0));
    assert!(nearly_equal(vehicle_takeover.board_time(), 8.0));
    assert!(nearly_equal(vehicle_takeover.revert_damage_fraction(), 0.5));
    assert!(vehicle_takeover.veterancy_override());
    assert_eq!(vehicle_takeover.board_animation(), Some("HijackIdle"));
    assert!(nearly_equal(vehicle_takeover.unjoin_max_distance(), 25.0));
    assert_eq!(vehicle_takeover.levels(), 1);
    assert!(nearly_equal(vehicle_takeover.damage_modifier(), 1.15));
    assert!(nearly_equal(vehicle_takeover.damage_taken_modifier(), 0.87));
    assert!(!vehicle_takeover.damage_by_combat_value());
    assert!(gameplay.squad_combat_value("unsc_inf_spartan_01") > 0.0);
    assert!(gameplay.squad_combat_value("unsc_veh_scorpion_01") > 0.0);
    assert!(
        gameplay
            .squad_veterancy_thresholds("unsc_inf_spartan_01")
            .is_some_and(|thresholds| !thresholds.is_empty())
    );
    assert_eq!(
        vehicle_takeover
            .attachment()
            .map(|attachment| attachment.name.as_str()),
        Some("fx_hijacked")
    );
    let merged = gameplay
        .merged_squad_profile("unsc_inf_spartan_01", "unsc_inf_marine_01")
        .expect("scenario-layered squads.xml should retain the Spartan/Marine merge table");
    assert_eq!(merged.joining_proto_squad_name(), "unsc_inf_spartan_01");
    assert_eq!(merged.target_proto_squad_name(), "unsc_inf_marine_01");
    assert_eq!(
        merged.proto_squad_name(),
        "merged_unsc_inf_marine_01_unsc_inf_spartan_01"
    );
    assert!(
        usize::try_from(merged.proto_squad_id()).unwrap() >= loaded.content.database.squads.len()
    );
}

fn assert_real_plasma_shield_lifecycle(loaded: &mut LoadedGameScenario, player_id: sim::PlayerId) {
    let world = &mut loaded.simulation.world;
    let base_id = world.create_base(player_id, glam::Vec3::new(4_600.0, 0.0, 4_600.0));
    let anchor_id = world.get_base(base_id).unwrap().anchor_building_id;
    let generator_id = world.create_building(player_id);
    "cov_bldg_shieldGen_01"
        .clone_into(&mut world.get_unit_mut(generator_id).unwrap().proto_object_name);
    assert!(world.add_building_to_base(base_id, generator_id));

    world.update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);

    let base = world.get_base(base_id).expect("synthetic protected base");
    assert_eq!(base.primary_plasma_shield_generator(), Some(generator_id));
    let shield_squad_id = base
        .plasma_shield_squad()
        .expect("real plasma shield squad");
    let shield_unit_id = world.get_squad(shield_squad_id).unwrap().unit_ids[0];
    let shield = world
        .get_unit(shield_unit_id)
        .expect("real plasma shield unit");
    assert_eq!(shield.proto_object_name, "cov_bldg_shield_01");
    assert!(nearly_equal(shield.shields.maximum, 50_000.0));
    assert!(nearly_equal(shield.obstruction_half_extents.x, 40.0));
    assert!(nearly_equal(shield.obstruction_half_extents.y, 6.0));
    assert!(nearly_equal(shield.obstruction_half_extents.z, 24.0));
    let anchor_squad_id = world.get_unit(anchor_id).unwrap().squad_id.unwrap();
    assert_eq!(
        world.get_squad(anchor_squad_id).unwrap().damage_proxy(),
        Some(shield_squad_id)
    );

    assert!(world.destroy_base(base_id));
    assert!(world.get_squad(shield_squad_id).is_none());
}

fn assert_real_bubble_shield_lifecycle(loaded: &mut LoadedGameScenario, player_id: sim::PlayerId) {
    let monitor_proto_id = squad_prototype_id(&loaded.content.database, "for_air_monitor_04")
        .expect("real database should expose the Protector monitor squad");
    let target_proto_id = squad_prototype_id(&loaded.content.database, "unsc_veh_scorpion_01")
        .expect("real database should expose the Scorpion squad");
    let target_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        target_proto_id,
        glam::Vec3::new(4_800.0, 0.0, 4_800.0),
        glam::Vec3::Z,
    )
    .expect("real Scorpion target should spawn");
    let monitor_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        monitor_proto_id,
        glam::Vec3::new(4_802.0, 0.0, 4_800.0),
        glam::Vec3::Z,
    )
    .expect("real Protector monitor should spawn");
    let world = &mut loaded.simulation.world;
    assert!(world.issue_join_order(player_id, monitor_squad_id, target_squad_id, None));
    world.update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);

    let bubble_squad_id = world
        .get_squad(monitor_squad_id)
        .and_then(sim::Squad::bubble_shield_squad)
        .expect("real Follow join should create its mapped bubble squad");
    let bubble_squad = world.get_squad(bubble_squad_id).unwrap();
    assert_eq!(bubble_squad.proto_squad_name, "sys_bubbleshield_med_01");
    let bubble_unit_id = bubble_squad.unit_ids[0];
    let bubble_unit = world.get_unit(bubble_unit_id).unwrap();
    assert_eq!(bubble_unit.proto_object_name, "sys_bubbleshield_med");
    assert!(nearly_equal(bubble_unit.shields.maximum, 5_000.0));
    assert!(bubble_unit.is_external_shield());
    assert_eq!(
        world.get_squad(target_squad_id).unwrap().damage_proxy(),
        Some(bubble_squad_id)
    );

    assert!(world.kill_squad(target_squad_id, true));
    assert!(world.get_squad(monitor_squad_id).is_none());
    assert!(world.get_squad(bubble_squad_id).is_none());
}

fn assert_real_follow_attack_lifecycle(loaded: &mut LoadedGameScenario, player_id: sim::PlayerId) {
    let enemy_player_id = loaded
        .simulation
        .world
        .active_players()
        .map(|player| player.id)
        .find(|&other| {
            loaded
                .simulation
                .world
                .players_are_enemies(player_id, other)
        })
        .expect("the skirmish scenario should contain an enemy player");
    let database = &loaded.content.database;
    let monitor_proto_id = squad_prototype_id(database, "for_air_monitor_01")
        .expect("real database should expose the attack Protector monitor");
    let target_proto_id = squad_prototype_id(database, "unsc_veh_scorpion_01")
        .expect("real database should expose the Scorpion squad");
    let target_position = glam::Vec3::new(4_850.0, 0.0, 4_850.0);
    let target_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        target_proto_id,
        target_position,
        glam::Vec3::Z,
    )
    .expect("real protected Scorpion should spawn");
    let monitor_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        monitor_proto_id,
        target_position + glam::Vec3::X * 0.25,
        glam::Vec3::Z,
    )
    .expect("real attack Protector should spawn");
    let enemy_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        enemy_player_id,
        target_proto_id,
        target_position + glam::Vec3::Z * 10.0,
        -glam::Vec3::Z,
    )
    .expect("real enemy Scorpion should spawn");
    let world = &mut loaded.simulation.world;
    assert!(world.issue_attack_order(player_id, target_squad_id, enemy_squad_id, 20.0));
    assert!(world.issue_join_order(player_id, monitor_squad_id, target_squad_id, None));
    world.update_entities_with_gameplay(0.55, &loaded.simulation.gameplay);

    let monitor = world.get_squad(monitor_squad_id).unwrap();
    assert_eq!(monitor.join_kind(), Some(JoinKind::FollowAttack));
    assert_eq!(monitor.attack_target, Some(enemy_squad_id));
    assert_eq!(monitor.bubble_shield_squad(), None);

    assert!(world.kill_squad(target_squad_id, true));
    assert!(world.get_squad(monitor_squad_id).is_none());
    assert!(world.kill_squad(enemy_squad_id, true));
}

fn assert_real_merge_lifecycle(loaded: &mut LoadedGameScenario, player_id: sim::PlayerId) {
    let database = &loaded.content.database;
    let spartan_proto_id = squad_prototype_id(database, "unsc_inf_spartan_01")
        .expect("real database should expose the Spartan squad");
    let marine_proto_id = squad_prototype_id(database, "unsc_inf_marine_01")
        .expect("real database should expose the Marine squad");
    let position = glam::Vec3::new(4_900.0, 0.0, 4_900.0);
    let marine_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        marine_proto_id,
        position,
        glam::Vec3::Z,
    )
    .expect("real Marine target should spawn");
    let spartan_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        spartan_proto_id,
        position + glam::Vec3::X,
        glam::Vec3::Z,
    )
    .expect("real Spartan source should spawn");
    let joining_unit_id = loaded
        .simulation
        .world
        .get_squad(spartan_squad_id)
        .unwrap()
        .unit_ids[0];
    let original_marine_count = loaded
        .simulation
        .world
        .get_squad(marine_squad_id)
        .unwrap()
        .unit_ids
        .len();
    let world = &mut loaded.simulation.world;
    assert!(world.issue_join_order(
        player_id,
        spartan_squad_id,
        marine_squad_id,
        loaded.simulation.gameplay.command_ability_id(),
    ));
    world.update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);

    assert!(
        world.get_squad(spartan_squad_id).is_none(),
        "real Merge source remained: {:?}",
        world.get_squad(spartan_squad_id).map(|squad| (
            squad.join_target(),
            squad.join_kind(),
            squad.join_merge_type(),
            squad.unit_ids.len(),
            squad.base.position,
        ))
    );
    let merged = world.get_squad(marine_squad_id).unwrap();
    assert_eq!(
        merged.proto_squad_name,
        "merged_unsc_inf_marine_01_unsc_inf_spartan_01"
    );
    assert_eq!(merged.unit_ids.len(), original_marine_count + 1);
    assert!(merged.contains_unit(joining_unit_id));
    assert_eq!(
        merged.merge_state().unwrap().joining_unit_id(),
        joining_unit_id
    );
    assert_eq!(
        world.get_unit(joining_unit_id).unwrap().squad_id,
        Some(marine_squad_id)
    );
    assert!(world.kill_squad(marine_squad_id, true));
}

fn assert_real_board_lifecycle(loaded: &mut LoadedGameScenario, player_id: sim::PlayerId) {
    let (enemy_player_id, ids) = spawn_real_board_pair(loaded, player_id);
    let [
        spartan_squad_id,
        target_squad_id,
        source_unit_id,
        target_unit_id,
    ] = ids;
    let command_ability_id = loaded.simulation.gameplay.command_ability_id();
    let world = &mut loaded.simulation.world;
    assert!(world.issue_join_order(
        player_id,
        spartan_squad_id,
        target_squad_id,
        command_ability_id,
    ));
    world.update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);
    assert!(world.get_unit(target_unit_id).unwrap().is_being_boarded());
    assert!(
        !world
            .get_squad(spartan_squad_id)
            .unwrap()
            .board_state()
            .unwrap()
            .is_complete()
    );

    for _ in 0..200 {
        world.update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);
        if world
            .get_squad(spartan_squad_id)
            .and_then(sim::Squad::board_state)
            .is_some_and(sim::SquadBoardState::is_complete)
        {
            break;
        }
    }

    let board = world
        .get_squad(spartan_squad_id)
        .and_then(sim::Squad::board_state)
        .expect("real Board action should remain persistent");
    assert!(board.is_complete());
    assert_eq!(board.former_owner(), enemy_player_id);
    assert!(board.veterancy_override());
    assert_eq!(board.levels(), 1);
    assert_eq!(
        board.effective_veterancy_bonus(),
        board.source_veterancy_level() + 1
    );
    assert_eq!(board.attachment_proto_object_name(), Some("fx_hijacked"));
    let attachment_id = board
        .attachment_entity_id()
        .expect("real Board action should create its scenario-layered attachment");
    let attachment = world
        .get_object(attachment_id)
        .expect("real Board attachment should remain authoritative sim state");
    assert_eq!(attachment.proto_object_name, "fx_hijacked");
    assert_eq!(attachment.object_state.attached_to(), Some(target_unit_id));
    assert_eq!(
        world.get_squad(target_squad_id).unwrap().base.player_id,
        player_id
    );
    assert_eq!(
        world.get_unit(target_unit_id).unwrap().base.player_id,
        player_id
    );
    assert_eq!(
        world
            .get_unit(source_unit_id)
            .unwrap()
            .garrison
            .container_id(),
        Some(target_unit_id)
    );
    assert!(world.add_squad_experience(target_squad_id, 1.0, &loaded.simulation.gameplay,));
    let source_experience = world.get_squad(spartan_squad_id).unwrap().experience();
    let target_experience = world.get_squad(target_squad_id).unwrap().experience();
    assert!(source_experience > 0.0);
    assert!(target_experience > 0.0);
    assert!(nearly_equal(source_experience + target_experience, 1.0));
    assert!(world.kill_squad(target_squad_id, true));
    assert!(world.get_squad(spartan_squad_id).is_some());
    assert!(
        world
            .get_unit(source_unit_id)
            .unwrap()
            .garrison
            .container_id()
            .is_none()
    );
    assert!(world.kill_squad(spartan_squad_id, true));
}

fn spawn_real_board_pair(
    loaded: &mut LoadedGameScenario,
    player_id: sim::PlayerId,
) -> (sim::PlayerId, [sim::EntityId; 4]) {
    let enemy_player_id = loaded
        .simulation
        .world
        .active_players()
        .map(|player| player.id)
        .find(|&other| {
            loaded
                .simulation
                .world
                .players_are_enemies(player_id, other)
        })
        .expect("the skirmish scenario should contain an enemy player");
    let database = &loaded.content.database;
    let spartan_proto_id = squad_prototype_id(database, "unsc_inf_spartan_01")
        .expect("real database should expose the Spartan squad");
    let target_proto_id = squad_prototype_id(database, "unsc_veh_scorpion_01")
        .expect("real database should expose the Scorpion squad");
    let position = glam::Vec3::new(4_950.0, 0.0, 4_950.0);
    let spartan_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        player_id,
        spartan_proto_id,
        position,
        glam::Vec3::Z,
    )
    .expect("real boarding Spartan should spawn");
    let target_squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        enemy_player_id,
        target_proto_id,
        position,
        glam::Vec3::Z,
    )
    .expect("real enemy Scorpion should spawn");
    let source_unit_id = loaded
        .simulation
        .world
        .get_squad(spartan_squad_id)
        .unwrap()
        .unit_ids[0];
    let target_unit_id = loaded
        .simulation
        .world
        .get_squad(target_squad_id)
        .unwrap()
        .unit_ids[0];
    (
        enemy_player_id,
        [
            spartan_squad_id,
            target_squad_id,
            source_unit_id,
            target_unit_id,
        ],
    )
}

pub(super) fn assert_real_external_shield_loading(loaded: &mut LoadedGameScenario) {
    let prototype_name = "env_generic_wallshield_01";
    let prototype = loaded
        .content
        .database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(prototype_name))
        .expect("real database should contain the generic wall shield");
    assert!(
        prototype
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("ExternalShield"))
    );
    let prototype_id = object_prototype_id(&loaded.content.database, prototype_name)
        .expect("real database should expose the wall-shield prototype ID");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("the scenario should assign a player base")
        .0;
    let unit_id = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        prototype_id,
        glam::Vec3::new(4_700.0, 0.0, 4_700.0),
        glam::Vec3::Z,
    )
    .expect("the scenario-layered wall shield should spawn as a building");
    let unit = loaded
        .simulation
        .world
        .get_unit(unit_id)
        .expect("spawned wall-shield unit");
    assert!(unit.is_external_shield());
    assert!(nearly_equal(unit.obstruction_half_extents.x, 0.5));
    assert!(nearly_equal(unit.obstruction_half_extents.y, 20.0));
    assert!(nearly_equal(unit.obstruction_half_extents.z, 39.0));
    assert!(loaded.simulation.world.remove_unit(unit_id).is_some());
}
