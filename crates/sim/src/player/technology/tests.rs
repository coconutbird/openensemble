use super::*;
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper};

#[test]
fn static_and_runtime_weapon_accuracy_effects_layer_in_retail_order() {
    let technology = Tech {
        name: "Sharpshooter".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![
                weapon_effect("Accuracy", 0.5, "Percent", Some("Rifle"), false),
                weapon_effect("MovingMaxDeviation", 2.0, "Percent", None, true),
                weapon_effect("MaxVelocityLead", 5.0, "Assign", None, true),
            ],
        }),
        ..Tech::default()
    };
    let mut database = Database::new();
    database.techs.push(technology.clone());
    let mut state = PlayerTechState::default();
    let _transforms = state.activate(&database, &technology);

    assert!((state.weapon_accuracy("Marine", "Rifle", 0.8) - 0.4).abs() < f32::EPSILON);
    assert!((state.weapon_accuracy("Marine", "Pistol", 0.8) - 0.8).abs() < f32::EPSILON);
    assert!((state.weapon_moving_max_deviation("Marine", "Rifle", 3.0) - 6.0).abs() < f32::EPSILON);
    assert!((state.weapon_max_velocity_lead("Marine", "Rifle", 0.0) - 5.0).abs() < f32::EPSILON);

    state.modify_proto_data(
        "Marine",
        &ProtoDataModification {
            data_type: ProtoDataType::Accuracy,
            amount: 2.0,
            relativity: ProtoDataRelativity::Percent,
            all_actions: false,
            name: Some("Rifle".to_owned()),
            invert: false,
            command_type: None,
            command_data: None,
        },
    );
    assert!((state.weapon_accuracy("Marine", "Rifle", 0.8) - 0.8).abs() < f32::EPSILON);
}

#[test]
fn player_damage_modifiers_are_pair_specific_and_rebuild_on_deactivation() {
    let multiplier = Tech {
        name: "DoublePlasmaVsHeavy".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![damage_modifier_effect(2.0, "Percent")],
        }),
        ..Tech::default()
    };
    let addition = Tech {
        name: "AddPlasmaVsHeavy".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![damage_modifier_effect(0.25, "Absolute")],
        }),
        ..Tech::default()
    };
    let mut database = Database::new();
    database
        .techs
        .extend([multiplier.clone(), addition.clone()]);
    let mut state = PlayerTechState::default();

    let _transforms = state.activate(&database, &multiplier);
    let _transforms = state.activate(&database, &addition);
    assert!(
        (state.weapon_type_damage_modifier("Plasma", "Heavy", 1.5) - 3.25).abs() < f32::EPSILON
    );
    assert!((state.weapon_type_damage_modifier("Plasma", "Light", 1.5) - 1.5).abs() < f32::EPSILON);

    assert!(state.deactivate(&database, "AddPlasmaVsHeavy"));
    assert!((state.weapon_type_damage_modifier("Plasma", "Heavy", 1.5) - 3.0).abs() < f32::EPSILON);
    assert!(state.deactivate(&database, "DoublePlasmaVsHeavy"));
    assert!((state.weapon_type_damage_modifier("Plasma", "Heavy", 1.5) - 1.5).abs() < f32::EPSILON);
}

#[test]
fn ammunition_effects_layer_with_runtime_proto_changes() {
    let technology = Tech {
        name: "MoreAmmo".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![
                proto_effect("AmmoMax", 1.25, "Percent"),
                proto_effect("AmmoRegenRate", 2.0, "Percent"),
            ],
        }),
        ..Tech::default()
    };
    let mut database = Database::new();
    database.techs.push(technology.clone());
    let mut state = PlayerTechState::default();
    let _transforms = state.activate(&database, &technology);

    assert_close(state.ammunition_maximum("Marine", 200.0), 250.0);
    assert_close(state.ammunition_regeneration_rate("Marine", 9.0), 18.0);
    state.modify_proto_data(
        "Marine",
        &ProtoDataModification {
            data_type: ProtoDataType::AmmoMax,
            amount: 2.0,
            relativity: ProtoDataRelativity::Percent,
            all_actions: true,
            name: None,
            invert: false,
            command_type: None,
            command_data: None,
        },
    );
    assert_close(state.ammunition_maximum("Marine", 200.0), 500.0);
}

#[test]
fn ram_weapon_caps_and_reflection_use_action_scoped_technology() {
    let technology = Tech {
        name: "RamUpgrade".to_owned(),
        effects: Some(EffectsWrapper {
            entries: vec![
                weapon_effect("MaxDamagePerRam", 0.5, "Percent", Some("Ram"), false),
                weapon_effect("ReflectDamageFactor", 0.1, "Absolute", Some("Ram"), false),
            ],
        }),
        ..Tech::default()
    };
    let mut database = Database::new();
    database.techs.push(technology.clone());
    let mut state = PlayerTechState::default();
    let _transforms = state.activate(&database, &technology);

    assert_close(
        state.weapon_max_damage_per_ram("Marine", "Ram", 10_000.0),
        5_000.0,
    );
    assert_close(
        state.weapon_reflect_damage_factor("Marine", "Ram", 0.4),
        0.5,
    );
    assert_close(
        state.weapon_max_damage_per_ram("Marine", "Rifle", 10_000.0),
        10_000.0,
    );
}

fn damage_modifier_effect(amount: f32, relativity: &str) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        subtype: Some("DamageModifier".to_owned()),
        amount: Some(amount),
        relativity: Some(relativity.to_owned()),
        weapon_type: Some("Plasma".to_owned()),
        damage_type: Some("Heavy".to_owned()),
        target: Some(EffectTarget {
            target_type: Some("Player".to_owned()),
            value: Some("Player".to_owned()),
        }),
        ..TechEffect::default()
    }
}

fn weapon_effect(
    subtype: &str,
    amount: f32,
    relativity: &str,
    action: Option<&str>,
    all_actions: bool,
) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        subtype: Some(subtype.to_owned()),
        amount: Some(amount),
        relativity: Some(relativity.to_owned()),
        action: action.map(str::to_owned),
        allactions: Some(all_actions),
        target: Some(EffectTarget {
            target_type: Some("ProtoUnit".to_owned()),
            value: Some("Marine".to_owned()),
        }),
        ..TechEffect::default()
    }
}

fn proto_effect(subtype: &str, amount: f32, relativity: &str) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        subtype: Some(subtype.to_owned()),
        amount: Some(amount),
        relativity: Some(relativity.to_owned()),
        target: Some(EffectTarget {
            target_type: Some("ProtoUnit".to_owned()),
            value: Some("Marine".to_owned()),
        }),
        ..TechEffect::default()
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}
