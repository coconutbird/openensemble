use std::collections::BTreeSet;

use glam::Vec3;
use sim::{
    EntityId, NativePowerError, NativePowerInvocation, PowerUserId, load_scenario_from_game_dir,
};

const SHIPPED_POWER_TYPES: [&str; 10] = [
    "CarpetBombing",
    "Cleansing",
    "Cryo",
    "Disruption",
    "ODST",
    "Orbital",
    "Rage",
    "Repair",
    "Transport",
    "Wave",
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-native-power-routing -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn every_scenario_layered_shipped_native_power_type_reaches_a_sim_implementation() {
    let mut loaded = load_installed_scenario();
    let routed_powers = loaded
        .content
        .database
        .powers
        .iter()
        .enumerate()
        .filter_map(|(index, power)| {
            let power_type = power.attributes.as_ref()?.power_type.as_deref()?.trim();
            (!power_type.is_empty()).then(|| {
                (
                    i32::try_from(index).expect("power prototype index"),
                    power.name.clone(),
                    power_type.to_owned(),
                )
            })
        })
        .collect::<Vec<_>>();
    let actual_types = routed_powers
        .iter()
        .map(|(_, _, power_type)| power_type.as_str())
        .collect::<BTreeSet<_>>();
    let expected_types = SHIPPED_POWER_TYPES.into_iter().collect::<BTreeSet<_>>();
    assert_eq!(actual_types, expected_types);

    for (proto_power_id, power_name, power_type) in routed_powers {
        let result = loaded.simulation.world.invoke_native_power(
            &loaded.content.database,
            NativePowerInvocation {
                player_id: u8::MAX,
                proto_power_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
                power_user_id: PowerUserId::INVALID,
            },
        );
        assert!(
            !matches!(result, Err(NativePowerError::UnsupportedPowerType(_))),
            "shipped power {power_name} uses unrouted type {power_type}"
        );
    }
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
