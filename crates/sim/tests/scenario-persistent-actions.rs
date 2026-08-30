use sim::{GameplayCatalog, load_scenario_from_game_dir};

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_persistent_actions_survive_scenario_layering() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered gameplay should load");
    let gameplay = &loaded.simulation.gameplay;

    assert_unit_persistent(gameplay, "cov_inf_hunter_01", "Dodge", "Dodge");
    assert_unit_persistent(gameplay, "cov_inf_hunter_01", "Deflect", "Deflect");
    assert_squad_persistent(gameplay, "cov_inf_hunter_01", "SpiritBond", "SpiritBond");
    assert_squad_persistent(
        gameplay,
        "cov_inf_arbiter_01",
        "FiendishReturn",
        "ReflectDamage",
    );
    assert_squad_persistent(gameplay, "cov_inf_arbiter_01", "Cloak", "Cloak");
    assert_squad_persistent(gameplay, "cov_inf_elitecommando_01", "Cloak", "Cloak");
    assert_squad_persistent(gameplay, "fld_air_sporecloud_01", "WanderAction", "Wander");
    assert_unit_persistent(gameplay, "fld_air_sporecloud_01", "InfectAction", "Infect");
    assert_unit_persistent(
        gameplay,
        "cov_inf_prophet_02",
        "ShieldAction",
        "EnergyShield",
    );
    assert_unit_persistent(
        gameplay,
        "cov_inf_elitecommando_01",
        "ShieldAction",
        "InfantryEnergyShield",
    );
    assert_bomb_profiles(gameplay);
    assert_air_traffic_profiles(gameplay);
    assert_squad_persistent(
        gameplay,
        "env_creatures_bird_01",
        "AmbientLife",
        "AmbientLife",
    );
    assert_unit_persistent(
        gameplay,
        "env_harvest_treepineicy_spawner_01",
        "AmbientLifeSpawner",
        "AmbientLifeSpawner",
    );
    assert_typed_persistent(
        gameplay,
        "cov_veh_wraith_01",
        "PlasmaMortarAttackAction",
        "SecondaryTurretAttack",
    );

    let defense = gameplay
        .projectile_defense("COV_INF_HUNTER_01")
        .expect("Hunter persistent projectile defenses");
    let dodge = defense.dodge().expect("Hunter Dodge profile");
    assert_eq!(dodge.action_name(), "Dodge");
    assert!(!dodge.starts_disabled());
    assert!(nearly_equal(dodge.chance_max(), 0.66));
    assert!(nearly_equal(dodge.chance_min(), 0.66));
    assert!(nearly_equal(dodge.max_angle(), std::f32::consts::FRAC_PI_2));
    assert!(nearly_equal(dodge.cooldown(), 3.0));
    let deflect = defense.deflect().expect("Hunter Deflect profile");
    assert_eq!(deflect.action_name(), "Deflect");
    assert!(deflect.starts_disabled());
    assert!(nearly_equal(deflect.chance_max(), 0.66));
    assert!(nearly_equal(
        deflect.max_angle(),
        std::f32::consts::FRAC_PI_2
    ));
    assert!(nearly_equal(deflect.cooldown(), 1.0));

    let fuel_rod = gameplay
        .object("cov_inf_hunter_01")
        .and_then(|object| {
            object
                .attack_profiles()
                .find(|profile| profile.weapon_name.eq_ignore_ascii_case("FuelRodCannon"))
        })
        .expect("Hunter FuelRodCannon attack timing");
    assert!(fuel_rod.projectile_reactions.dodgeable());
    assert!(fuel_rod.projectile_reactions.deflectable());
    assert!(!fuel_rod.projectile_reactions.small_arms_deflectable());

    let reflection = gameplay
        .reflect_damage("cov_inf_arbiter_01")
        .expect("Arbiter FiendishReturn profile");
    assert_eq!(reflection.action_name(), "FiendishReturn");
    assert!(reflection.starts_disabled());
    assert!(nearly_equal(reflection.work_rate(), 0.15));

    let cloak = gameplay
        .cloak("cov_inf_elitecommando_01")
        .expect("Elite Commando Cloak profile");
    assert!(!cloak.permanent());
    assert!(cloak.move_while_cloaked());
    assert!(nearly_equal(cloak.cloaking_delay(), 1.0));
    assert!(nearly_equal(cloak.recloak_delay(), 5.0));

    let wander = gameplay
        .wander("fld_air_sporecloud_01")
        .expect("Flood spore-cloud Wander profile");
    assert_eq!(wander.action_name(), "WanderAction");
    assert!(nearly_equal(wander.work_range(), 50.0));
    assert!(!wander.starts_disabled());

    assert_ambient_profiles(gameplay);
}

fn assert_bomb_profiles(gameplay: &GameplayCatalog) {
    assert_unit_persistent(gameplay, "fx_proj_fldbomb_01", "Bomb", "Bomb");
    assert_unit_persistent(gameplay, "fx_proj_fldbomb_01", "Detonate", "Detonate");
}

fn assert_air_traffic_profiles(gameplay: &GameplayCatalog) {
    for prototype in ["unsc_bldg_airPad_01", "cov_bldg_heavyfactory_01"] {
        assert_unit_persistent(
            gameplay,
            prototype,
            "AirTrafficControl",
            "AirTrafficControl",
        );
        let profiles = gameplay.air_traffic_control_actions(prototype);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].action_name(), "AirTrafficControl");
        assert!(!profiles[0].starts_disabled());
    }
}

fn assert_ambient_profiles(gameplay: &GameplayCatalog) {
    let ambient = gameplay
        .ambient_life("env_creatures_bird_01")
        .expect("bird AmbientLife profile");
    assert_eq!(ambient.action_name(), "AmbientLife");
    assert!(nearly_equal(ambient.max_wander_frequency(), 20.0));
    assert!(nearly_equal(ambient.predator_check_frequency(), 2.0));
    assert!(nearly_equal(ambient.prey_check_frequency(), 0.0));
    assert!(nearly_equal(ambient.opportunity_check_radius(), 20.0));
    assert!(nearly_equal(ambient.flee_distance(), 40.0));
    assert!(nearly_equal(ambient.flee_movement_modifier(), 1.5));
    assert!(nearly_equal(ambient.minimum_wander_distance(), 30.0));
    assert!(nearly_equal(ambient.maximum_wander_distance(), 200.0));
    assert!(!ambient.starts_disabled());

    let spawner = gameplay
        .ambient_life_spawner("env_harvest_treepineicy_spawner_01")
        .expect("tree AmbientLifeSpawner profile");
    assert_eq!(spawner.action_name(), "AmbientLifeSpawner");
    assert_eq!(spawner.squad_type(), "env_creatures_bird_01");
    assert!(nearly_equal(spawner.check_frequency(), 1.0));
    assert!(nearly_equal(spawner.opportunity_check_radius(), 20.0));
    assert!(!spawner.starts_disabled());
}

fn assert_unit_persistent(
    gameplay: &GameplayCatalog,
    proto_object: &str,
    action_name: &str,
    action_type: &str,
) {
    let object = gameplay
        .object(proto_object)
        .expect("layered object tactics");
    let tactics = object.tactics();
    let rules = tactics.tactic.as_ref().expect("retail tactic rules");
    assert!(
        rules
            .persistent_actions
            .iter()
            .any(|name| name.eq_ignore_ascii_case(action_name))
    );
    let action = tactics
        .actions
        .iter()
        .find(|action| action.name.eq_ignore_ascii_case(action_name))
        .expect("named persistent unit action");
    assert!(
        action
            .action_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case(action_type))
    );
}

fn assert_squad_persistent(
    gameplay: &GameplayCatalog,
    proto_object: &str,
    action_name: &str,
    action_type: &str,
) {
    let object = gameplay
        .object(proto_object)
        .expect("layered object tactics");
    let tactics = object.tactics();
    let rules = tactics.tactic.as_ref().expect("retail tactic rules");
    assert!(
        rules
            .persistent_squad_actions
            .iter()
            .any(|name| name.eq_ignore_ascii_case(action_name))
    );
    let action = tactics
        .actions
        .iter()
        .find(|action| action.name.eq_ignore_ascii_case(action_name))
        .expect("named persistent squad action");
    assert!(
        action
            .action_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case(action_type))
    );
}

fn assert_typed_persistent(
    gameplay: &GameplayCatalog,
    proto_object: &str,
    action_name: &str,
    persistent_type: &str,
) {
    let action = gameplay
        .object(proto_object)
        .expect("layered object tactics")
        .tactics()
        .actions
        .iter()
        .find(|action| action.name.eq_ignore_ascii_case(action_name))
        .expect("typed persistent action");
    assert!(
        action
            .persistent_action_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case(persistent_type))
    );
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.000_1
}
