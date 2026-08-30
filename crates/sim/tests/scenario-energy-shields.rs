use glam::Vec3;
use sim::{
    EnergyShieldPhase, EnergyShieldPresentationKind, EnergyShieldVisualProfile, GameplayCatalog,
    World, load_scenario_from_game_dir, object_prototype_id, spawn_object_at, spawn_squad_at,
    squad_prototype_id,
};

const PROPHET: &str = "cov_inf_prophet_02";
const PROPHET_SHIELD: &str = "cov_prophetshield3";
const ELITE_COMMANDO: &str = "cov_inf_elitecommando_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-energy-shields -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_shield_actions_drive_authoritative_visual_state() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    assert_shipped_profiles(&loaded.simulation.gameplay, &loaded.content.database);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let position = scenario_center(world);
    assert_external_prophet_lifecycle(world, database, gameplay, position);
    assert_infantry_component_state(world, database, gameplay, position + Vec3::X * 10.0);
}

fn assert_shipped_profiles(
    gameplay: &GameplayCatalog,
    database: &pipeline::database::hw1::Database,
) {
    let prophet = gameplay.energy_shield_actions(PROPHET);
    assert_eq!(prophet.len(), 1);
    assert_eq!(prophet[0].action_name(), "ShieldAction");
    assert!(!prophet[0].starts_disabled());
    assert_eq!(
        prophet[0].visual(),
        &EnergyShieldVisualProfile::Attachment {
            prototype_name: PROPHET_SHIELD.to_owned(),
            prototype_id: object_prototype_id(database, PROPHET_SHIELD),
            bone_name: Some("Bone_Chair".to_owned()),
        }
    );

    let prophet_one = gameplay.energy_shield_actions("cov_inf_prophet_01");
    assert_eq!(prophet_one.len(), 2);
    assert_eq!(prophet_one[0].action_name(), "ShieldAction");
    assert_eq!(prophet_one[1].action_name(), "ShieldAction2");
    assert!(prophet_one[1].starts_disabled());

    for (prototype, shield) in [
        ("cov_veh_ghost_01", "cov_ghostshield"),
        ("cov_veh_locust_01", "cov_locustshield"),
        ("cov_veh_wraith_01", "cov_wraithshield"),
    ] {
        let profile = &gameplay.energy_shield_actions(prototype)[0];
        assert_eq!(profile.action_name(), "ShieldAction");
        assert_eq!(
            profile.visual(),
            &EnergyShieldVisualProfile::Attachment {
                prototype_name: shield.to_owned(),
                prototype_id: object_prototype_id(database, shield),
                bone_name: None,
            }
        );
    }

    let infantry = &gameplay.energy_shield_actions(ELITE_COMMANDO)[0];
    assert_eq!(infantry.action_name(), "ShieldAction");
    assert_eq!(
        infantry.visual(),
        &EnergyShieldVisualProfile::Infantry {
            component_name: "Shield".to_owned(),
            hit_duration_ms: 1_000,
        }
    );
}

fn assert_external_prophet_lifecycle(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    position: Vec3,
) {
    let unit_id = spawn_object_at(
        world,
        database,
        1,
        object_prototype_id(database, PROPHET).expect("shipped Prophet object"),
        position,
        Vec3::Z,
    )
    .expect("shipped Prophet should spawn");
    let maximum = world.get_unit(unit_id).unwrap().shields.maximum;
    world
        .get_unit_mut(unit_id)
        .unwrap()
        .shields
        .set_current(maximum);

    tick(world, database, gameplay);
    let action = shield_action(world, unit_id);
    assert_eq!(action.kind(), EnergyShieldPresentationKind::Attachment);
    assert_eq!(action.phase(), EnergyShieldPhase::Up);
    assert_eq!(action.bone_name(), Some("Bone_Chair"));
    let attachment_id = action.attachment_entity_id().expect("raised shield visual");
    assert_eq!(
        world.get_object(attachment_id).unwrap().proto_object_name,
        PROPHET_SHIELD
    );
    assert_animation(world, attachment_id, "Idle");

    assert!(world.damage_unit(unit_id, 1.0));
    tick(world, database, gameplay);
    assert_eq!(
        shield_action(world, unit_id).phase(),
        EnergyShieldPhase::Hit
    );
    assert_animation(world, attachment_id, "Incoming");

    let remaining_shields = world.unit_health(unit_id).unwrap().shieldpoints;
    assert!(world.damage_unit(unit_id, remaining_shields));
    tick(world, database, gameplay);
    assert_eq!(
        shield_action(world, unit_id).phase(),
        EnergyShieldPhase::Lowering
    );
    assert_animation(world, attachment_id, "Death");
    for _ in 0..100 {
        if shield_action(world, unit_id).phase() == EnergyShieldPhase::Down {
            break;
        }
        tick(world, database, gameplay);
    }
    assert_eq!(
        shield_action(world, unit_id).phase(),
        EnergyShieldPhase::Down
    );
    assert!(world.get_object(attachment_id).is_none());
}

fn assert_infantry_component_state(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    position: Vec3,
) {
    let squad_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, ELITE_COMMANDO).expect("shipped Elite Commando squad"),
        position,
        Vec3::Z,
    )
    .expect("shipped Elite Commando should spawn");
    let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.shields.maximum = 10.0;
    unit.shields.set_current(10.0);

    tick(world, database, gameplay);
    let action = shield_action(world, unit_id);
    assert_eq!(
        action.kind(),
        EnergyShieldPresentationKind::InfantryComponent
    );
    assert_eq!(action.phase(), EnergyShieldPhase::Down);
    assert!(
        !world
            .get_unit(unit_id)
            .unwrap()
            .visual_mesh_mask()
            .is_component_visible("Shield")
    );

    assert!(world.damage_unit(unit_id, 1.0));
    tick(world, database, gameplay);
    assert_eq!(
        shield_action(world, unit_id).phase(),
        EnergyShieldPhase::Hit
    );
    assert!(
        shield_action(world, unit_id)
            .transition_seconds_remaining()
            .is_some_and(|remaining| remaining > 0.9)
    );
}

fn shield_action(world: &World, unit_id: sim::EntityId) -> &sim::UnitEnergyShieldAction {
    world
        .get_unit(unit_id)
        .unwrap()
        .shields
        .energy_shield_actions()
        .first()
        .expect("connected persistent shield action")
}

fn assert_animation(world: &World, entity_id: sim::EntityId, expected: &str) {
    let animation = world
        .get_object(entity_id)
        .unwrap()
        .object_state
        .scripted_animation()
        .expect("scenario-loaded shield animation");
    assert_eq!(animation.animation_type(), expected);
    assert!(animation.asset_path().is_some());
    assert!(animation.duration_ms() > 0);
}

fn scenario_center(world: &World) -> Vec3 {
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut position = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    position.y = world.terrain_height(position, true).unwrap_or_default();
    position
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.advance_time(50);
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}
