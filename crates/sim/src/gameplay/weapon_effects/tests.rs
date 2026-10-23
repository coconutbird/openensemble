use super::*;
use pipeline::database::hw1::tactics::{Daze, DotEffect, Stasis};

fn action() -> Action {
    Action {
        name: "Attack".to_owned(),
        ..Action::default()
    }
}

#[test]
fn dot_effect_text_sets_every_size_and_attributes_replace_one() {
    let weapon = Weapon {
        dot_rate: Some(6.8),
        dot_duration: Some(1.0),
        dot_effect: Some(DotEffect {
            small: Some("fx_small".to_owned()),
            medium: None,
            large: Some(String::new()),
            default_effect: "fx_default".to_owned(),
        }),
        ..Weapon::default()
    };
    let profile = WeaponEffectProfile::from_action(&action(), &weapon).unwrap();
    let dot = profile.damage_over_time.unwrap();
    assert_eq!(dot.rate.to_bits(), 6.8_f32.to_bits());
    assert_eq!(dot.duration.to_bits(), 1.0_f32.to_bits());
    let sizes = DotEffectSizes {
        small: 4.0,
        medium: 7.0,
    };
    assert_eq!(dot.effect.for_radius(4.0, sizes), Some("fx_small"));
    assert_eq!(dot.effect.for_radius(4.5, sizes), Some("fx_default"));
    assert_eq!(dot.effect.for_radius(7.5, sizes), None);
}

#[test]
fn a_rate_that_is_not_positive_has_no_damage_over_time() {
    let weapon = Weapon {
        dot_rate: Some(0.0),
        dot_duration: Some(10.0),
        ..Weapon::default()
    };
    assert_eq!(WeaponEffectProfile::from_action(&action(), &weapon), None);
}

#[test]
fn stasis_needs_smart_targeting_but_drain_and_bomb_do_not() {
    let mut weapon = Weapon {
        stasis: Some(Stasis {
            smart_targeting: None,
        }),
        ..Weapon::default()
    };
    assert_eq!(WeaponEffectProfile::from_action(&action(), &weapon), None);
    weapon.stasis_bomb = Some(true);
    weapon.stasis_heal_to_drain_ratio = Some(1.0);
    let linked = Action {
        linked_action: Some("StasisBombAction".to_owned()),
        ..action()
    };
    let profile = WeaponEffectProfile::from_action(&linked, &weapon).unwrap();
    assert!(!profile.stasis.stasis);
    assert!(profile.stasis.bomb);
    assert!(profile.stasis.applies());
    assert_eq!(profile.smart_target, None);
    assert_eq!(
        profile.stasis.linked_action.as_deref(),
        Some("StasisBombAction")
    );
    weapon.stasis = Some(Stasis {
        smart_targeting: Some(true),
    });
    let profile = WeaponEffectProfile::from_action(&linked, &weapon).unwrap();
    assert!(profile.stasis.stasis);
    assert_eq!(profile.smart_target, Some(SmartTargetType::Stasis));
}

#[test]
fn daze_duration_is_milliseconds_and_area_daze_defaults_to_true() {
    let weapon = Weapon {
        daze: Some(Daze {
            target_type: Some("infantry".to_owned()),
            movement_modifier: None,
            aoe_daze: None,
            smart_targeting: Some(true),
            duration: 1.5,
        }),
        ..Weapon::default()
    };
    let profile = WeaponEffectProfile::from_action(&action(), &weapon).unwrap();
    let daze = profile.daze.unwrap();
    assert_eq!(daze.duration_ms.to_bits(), 1_500.0_f32.to_bits());
    assert_eq!(daze.movement_modifier.to_bits(), 0.0_f32.to_bits());
    assert!(daze.aoe);
    assert_eq!(daze.target_type.as_deref(), Some("infantry"));
    assert_eq!(profile.smart_target, Some(SmartTargetType::Daze));
}
