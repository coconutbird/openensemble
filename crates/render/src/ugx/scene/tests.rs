use std::collections::HashMap;

use glam::Vec3;
use pipeline::database::hw1::{Database, GameData, ProtoObject};
use pipeline::source::{AssetSource, StdFileProvider};

use super::{
    prototype_is_hidden, simulation_entity_flash, simulation_entity_transform,
    simulation_entity_visible_to_team, simulation_proto_names, simulation_unit_transform,
};

#[test]
fn simulation_transform_uses_only_live_position_and_facing() {
    let mut world = sim::World::new();
    let entity_id = world.create_unit_at(0, Vec3::new(12.0, 3.0, 56.0));
    let unit = world.get_unit_mut(entity_id).expect("new unit");
    unit.base.set_forward(Vec3::X);

    let transform = simulation_unit_transform(unit).expect("valid transform");
    assert!(
        transform
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(Vec3::new(12.0, 3.0, 56.0), 1.0e-6)
    );
    assert!(
        transform
            .transform_vector3(Vec3::Z)
            .abs_diff_eq(Vec3::X, 1.0e-6)
    );
    assert!(transform.determinant() > 0.0);
}

#[test]
fn invalid_simulation_facing_has_no_render_transform() {
    let mut world = sim::World::new();
    let entity_id = world.create_unit_at(0, Vec3::ZERO);
    let unit = world.get_unit_mut(entity_id).expect("new unit");
    unit.base.forward = Vec3::ZERO;

    assert!(simulation_unit_transform(unit).is_none());
}

#[test]
fn containment_state_alone_controls_simulation_visibility() {
    let mut world = sim::World::new();
    world.init_players(1);
    let container_squad = world.create_squad_at(0, Vec3::ZERO);
    let container_unit = world.create_building_at(0, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(container_unit, container_squad));
    world.get_unit_mut(container_unit).unwrap().garrison =
        sim::UnitGarrison::container(0.0, false, false, Vec::new());
    let passenger_squad = world.create_squad_at(1, Vec3::ZERO);
    let passenger_unit = world.create_unit_at(1, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(passenger_unit, passenger_squad));

    world
        .issue_garrison_order(1, passenger_squad, container_squad, 0.0)
        .expect("garrison command");
    world.advance_time(50);
    world.update_entities(0.05);
    assert!(world.get_unit(passenger_unit).unwrap().is_garrisoned());
    assert!(simulation_unit_transform(world.get_unit(passenger_unit).unwrap()).is_none());

    world
        .issue_ungarrison_order(1, passenger_squad, None)
        .expect("ungarrison command");
    assert!(simulation_unit_transform(world.get_unit(passenger_unit).unwrap()).is_none());
    world.advance_time(50);
    world.update_entities(0.05);
    assert!(simulation_unit_transform(world.get_unit(passenger_unit).unwrap()).is_some());
}

#[test]
fn prototype_no_render_flag_is_case_insensitive() {
    let hidden = ProtoObject {
        flags: vec!["ForceToGaiaPlayer".to_owned(), "nOrEnDeR".to_owned()],
        ..ProtoObject::default()
    };
    assert!(prototype_is_hidden(&hidden));
    assert!(!prototype_is_hidden(&ProtoObject::default()));
}

#[test]
fn roster_matching_tracks_generational_sim_entity_ids() {
    let mut world = sim::World::new();
    let first = world.create_unit(0);
    let state = |id, animation_revision| super::SimulationEntityState {
        id,
        animation_revision,
        visual_variation_index: None,
        visual_mesh_revision: 0,
        combat_animation: None,
    };
    let mut scene = super::UnitScene {
        simulation_entity_states: vec![state(first, 0)],
        simulation_entity_count: 1,
        ..super::UnitScene::default()
    };
    assert!(scene.roster_matches(&world));

    world.remove_unit(first).expect("first unit");
    let replacement = world.create_unit(0);
    assert_ne!(first, replacement);
    assert!(!scene.roster_matches(&world));

    scene.simulation_entity_states = vec![state(replacement, 0)];
    assert!(scene.roster_matches(&world));

    assert!(world.play_entity_animation(replacement, "Death".to_owned(), None, 1_000));
    assert!(!scene.roster_matches(&world));
    scene.simulation_entity_states = vec![state(replacement, 1)];
    assert!(scene.roster_matches(&world));
}

#[test]
fn runtime_no_render_state_removes_and_restores_the_sim_projection() {
    let mut world = sim::World::new();
    let unit_id = world.create_unit(0);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = "drop_unit".to_owned();
    assert_eq!(
        simulation_proto_names(&world).collect::<Vec<_>>(),
        ["drop_unit"]
    );
    assert!(simulation_entity_transform(&world, unit_id).is_some());

    assert!(world.set_entity_render_enabled(unit_id, false));
    assert!(simulation_proto_names(&world).next().is_none());
    assert!(simulation_entity_transform(&world, unit_id).is_none());

    assert!(world.set_entity_render_enabled(unit_id, true));
    assert_eq!(
        simulation_proto_names(&world).collect::<Vec<_>>(),
        ["drop_unit"]
    );
    assert!(simulation_entity_transform(&world, unit_id).is_some());
}

#[test]
fn visual_cache_distinguishes_random_explicit_and_animated_variants() {
    let idle = super::UnitAnimationRequest::default();
    let random = super::visual_cache_key("marine", None, idle);
    let first = super::visual_cache_key("marine", Some(0), idle);
    let second = super::visual_cache_key("marine", Some(1), idle);
    let animated = super::visual_cache_key(
        "marine",
        Some(1),
        super::UnitAnimationRequest {
            animation_type: Some("Attack"),
            animation_asset: Some("Art\\Attack.UAX"),
            uses_simulation_clock: true,
            ..idle
        },
    );
    let death = super::visual_cache_key(
        "marine",
        Some(1),
        super::UnitAnimationRequest {
            animation_type: Some("Death"),
            animation_asset: Some("Art\\Attack.UAX"),
            uses_simulation_clock: true,
            ..idle
        },
    );
    let moving_attack = super::visual_cache_key(
        "marine",
        Some(1),
        super::UnitAnimationRequest {
            animation_type: Some("Attack"),
            animation_asset: Some("Art\\Attack.UAX"),
            uses_simulation_clock: true,
            movement_animation_type: Some("Walk"),
            movement_animation_roll: 7,
            ..idle
        },
    );

    assert_ne!(random, first);
    assert_ne!(first, second);
    assert_ne!(second, animated);
    assert_ne!(animated, death);
    assert_ne!(animated, moving_attack);
}

#[test]
fn scenario_visual_variation_is_projected_only_from_sim_state() {
    let scenario = sim::ScenarioData::from_xml_str(
        r#"<Scenario><Objects>
                <Object ID="7" VisualVariationIndex="2">variation_crate</Object>
            </Objects></Scenario>"#,
    )
    .expect("valid scenario");
    let database = Database {
        objects: vec![ProtoObject {
            name: "variation_crate".to_owned(),
            visual: Some("variation_crate".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let loaded = sim::load_scenario_into_world(&scenario, &database);
    let entity_id = loaded.get_entity_id(7).unwrap();
    let projected = super::simulation_visuals(&loaded.world)
        .find(|visual| visual.id == entity_id)
        .expect("class-zero visual should enter the sim projection");

    assert_eq!(projected.visual_variation_index, Some(2));
    assert_eq!(
        super::simulation_entity_states(&loaded.world, None).collect::<Vec<_>>(),
        vec![super::SimulationEntityState {
            id: entity_id,
            animation_revision: 0,
            visual_variation_index: Some(2),
            visual_mesh_revision: 0,
            combat_animation: None,
        }]
    );
}

#[test]
fn scene_sync_refreshes_roster_once_per_sim_change() {
    let mut source = AssetSource::with_provider(StdFileProvider);
    let mut world = sim::World::new();
    let visuals = HashMap::new();
    let proto_objects = Vec::new();
    let mut scene = super::UnitScene::load_world(&mut source, &world, &visuals, &proto_objects);
    assert!(!scene.sync_world(&mut source, &world, &visuals, &proto_objects));

    let entity_id = world.create_unit(0);
    world
        .get_unit_mut(entity_id)
        .expect("unit")
        .proto_object_name = "missing_visual".to_owned();
    assert!(scene.sync_world(&mut source, &world, &visuals, &proto_objects));
    assert_eq!(scene.simulation_entity_count(), 1);
    assert_eq!(scene.placement_count(), 0);
    assert!(!scene.sync_world(&mut source, &world, &visuals, &proto_objects));
}

#[test]
fn invisible_sim_control_objects_do_not_pollute_the_visual_roster() {
    let mut source = AssetSource::with_provider(StdFileProvider);
    let mut world = sim::World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().team_id = 1;
    let database = Database {
        objects: vec![ProtoObject {
            name: "sys_revealer".to_owned(),
            dbid: Some(13),
            los: Some(1.0),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            minimum_revealer_size: Some(4.0),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let visuals = HashMap::new();
    let scene = super::UnitScene::load_world(&mut source, &world, &visuals, &database.objects);
    let revealer = world
        .create_revealer(&database, 1, Vec3::ZERO, 10.0, None)
        .expect("revealer");

    assert!(world.get_revealer(revealer).is_some());
    assert!(world.is_position_revealed_to_team(1, Vec3::new(9.0, 0.0, 0.0)));
    assert!(scene.roster_matches(&world));
    assert_eq!(scene.simulation_entity_count(), 0);
    assert_eq!(scene.placement_count(), 0);
}

#[test]
fn team_visibility_projection_reads_only_authoritative_sim_state() {
    let mut world = sim::World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    let friendly = world.create_unit_at(1, Vec3::ZERO);
    let enemy = world.create_unit_at(2, Vec3::new(100.0, 0.0, 100.0));

    assert!(simulation_entity_visible_to_team(&world, 1, friendly));
    assert!(!simulation_entity_visible_to_team(&world, 1, enemy));
    world.set_fog_of_war_enabled(false);
    assert!(simulation_entity_visible_to_team(&world, 1, enemy));
}

#[test]
fn flash_projection_reads_only_authoritative_sim_state() {
    let mut world = sim::World::new();
    let entity_id = world.create_unit(1);
    assert!(simulation_entity_flash(&world, entity_id).is_none());

    assert!(world.flash_entity(entity_id, 500, 3_000, [255, 255, 0, 255], 80.0));
    let flash = simulation_entity_flash(&world, entity_id).expect("sim flash request");
    assert_eq!(flash.color(), [255, 255, 0, 255]);
    assert_eq!(flash.expires_at_ms(), Some(3_000));
    assert_eq!(flash.scroll_speed().to_bits(), (-4.0_f32).to_bits());
    assert_eq!(flash.intensity().to_bits(), 80.0_f32.to_bits());
}
