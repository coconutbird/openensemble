use super::*;
use pipeline::database::hw1::tactics::EndAnimationRef;

fn animation(weight: i32, duration: f32, attacks: usize) -> AttackAnimation {
    AttackAnimation {
        asset_path: format!("attack_{duration}.uax"),
        weight,
        duration,
        attack_positions: vec![0.5; attacks],
        events: Vec::new(),
        hardpoint_track: None,
    }
}

#[test]
fn weighted_values_match_retail_attack_info_math() {
    let variants = [animation(1, 1.0, 1), animation(3, 2.0, 2)];
    assert!(
        (weighted_average(&variants, |variant| variant.duration).unwrap() - 1.75).abs() < 0.001
    );
    assert!(
        (weighted_average(&variants, |variant| {
            variant.attack_positions.len().to_f32().unwrap_or(f32::MAX)
        })
        .unwrap()
            - 1.75)
            .abs()
            < 0.001
    );
}

#[test]
fn canonical_asset_paths_preserve_existing_prefixes_and_extensions() {
    assert_eq!(
        canonical_visual_path("unsc/marine.vis"),
        "art\\unsc\\marine.vis"
    );
    assert_eq!(
        canonical_visual_path("art\\unsc\\marine.vis"),
        "art\\unsc\\marine.vis"
    );
    assert_eq!(
        canonical_animation_path("unsc/marine_attack"),
        "art\\unsc\\marine_attack.uax"
    );
    assert_eq!(
        canonical_animation_path("art\\unsc\\marine_attack.uax"),
        "art\\unsc\\marine_attack.uax"
    );
}

#[test]
fn area_damage_profile_preserves_authored_weapon_contract() {
    let weapon = Weapon {
        aoe_radius: Some(4.0),
        aoe_primary_target_factor: Some(0.25),
        aoe_distance_factor: Some(0.5),
        aoe_damage_factor: Some(0.2),
        aoe_linear_damage: Some(true),
        aoe_ignores_y_axis: Some(true),
        allow_friendly_fire: Some(true),
        ..Weapon::default()
    };
    assert_eq!(
        area_damage_profile(&weapon),
        Some(AreaDamageProfile {
            radius: 4.0,
            primary_target_factor: 0.25,
            distance_factor: 0.5,
            damage_factor: 0.2,
            linear_damage: true,
            ignores_y_axis: true,
            friendly_fire: true,
        })
    );
    assert_eq!(area_damage_profile(&Weapon::default()), None);
}

#[test]
fn projectile_reactions_preserve_all_three_weapon_permissions() {
    let flags = projectile_reaction_flags(&Weapon {
        dodgeable: Some(true),
        deflectable: Some(true),
        small_arms_deflectable: Some(true),
        ..Weapon::default()
    });

    assert!(flags.dodgeable());
    assert!(flags.deflectable());
    assert!(flags.small_arms_deflectable());
}

#[test]
fn pull_profile_preserves_range_exclusions_animation_and_velocity() {
    let action = Action {
        invalid_targets: vec![" scarab ".to_owned(), String::new()],
        end_anim: Some(EndAnimationRef {
            name: " Flail ".to_owned(),
            ..EndAnimationRef::default()
        }),
        velocity_scalar: Some(40.0),
        ..Action::default()
    };
    let weapon = Weapon {
        pull_units: Some(true),
        max_pull_range: Some(55.0),
        ..Weapon::default()
    };

    let pull = pull_attack_profile(&action, &weapon).expect("PullUnits profile");
    assert!((pull.max_range - 55.0).abs() < f32::EPSILON);
    assert_eq!(pull.invalid_targets, ["scarab"]);
    assert_eq!(pull.end_animation_type.as_deref(), Some("Flail"));
    assert!((pull.velocity_scalar - 40.0).abs() < f32::EPSILON);
    assert_eq!(pull_attack_profile(&action, &Weapon::default()), None);
}
