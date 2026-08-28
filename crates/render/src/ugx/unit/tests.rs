use glam::{Mat4, Vec3};
use pipeline::database::hw1::Visual;
use pipeline::database::hw1::visual::{Asset, Component, Logic, LogicEntry, Model as VisualModel};

use super::{
    UnitAttachmentKind, UnitAttachmentTrigger, UnitLoadError, attachment_transform,
    canonical_animation_path, canonical_model_path, model_asset_path,
};

#[test]
fn shipped_attachment_kinds_are_classified_strictly() {
    for (authored, expected) in [
        ("ModelRef", UnitAttachmentKind::ModelReference),
        ("ModelFile", UnitAttachmentKind::Model),
        ("ParticleFile", UnitAttachmentKind::Particle),
        ("TerrainEffect", UnitAttachmentKind::TerrainEffect),
        ("LightFile", UnitAttachmentKind::Light),
    ] {
        assert_eq!(
            UnitAttachmentKind::from_authored(authored).unwrap(),
            expected
        );
    }
    assert!(matches!(
        UnitAttachmentKind::from_authored("UnknownAttachment"),
        Err(UnitLoadError::UnsupportedAttachmentType(value))
            if value == "UnknownAttachment"
    ));
}

#[test]
fn animation_attachments_preserve_authored_trigger_lifetime() {
    assert!(UnitAttachmentTrigger::Persistent.matches_animation(None));
    assert!(UnitAttachmentTrigger::Persistent.matches_animation(Some("Idle")));

    let trigger = UnitAttachmentTrigger::Animation("Death".to_owned());
    assert!(trigger.matches_animation(Some("death")));
    assert!(!trigger.matches_animation(Some("Idle")));
    assert!(!trigger.matches_animation(None));
}

#[test]
fn model_paths_are_resolved_from_art() {
    assert_eq!(
        canonical_model_path("unsc/vehicle/warthog_01/wheel_01"),
        "art\\unsc\\vehicle\\warthog_01\\wheel_01"
    );
    assert_eq!(
        canonical_model_path("art\\unsc\\vehicle\\warthog_01\\wheel_01"),
        "art\\unsc\\vehicle\\warthog_01\\wheel_01"
    );
    assert_eq!(
        canonical_animation_path("unsc/vehicle/warthog_01/idle_01"),
        "art\\unsc\\vehicle\\warthog_01\\idle_01"
    );
}

#[test]
fn attachment_aligns_child_from_bone_to_parent_to_bone() {
    let to_bone = Mat4::from_translation(Vec3::new(10.0, 2.0, -3.0));
    let from_bone = Mat4::from_translation(Vec3::new(4.0, 1.0, -1.0));
    let transform = attachment_transform(Some(to_bone), Some(from_bone));
    let aligned = transform.transform_point3(from_bone.transform_point3(Vec3::ZERO));
    let expected = to_bone.transform_point3(Vec3::ZERO);
    assert!(aligned.abs_diff_eq(expected, 1.0e-5));
}

#[test]
fn scenario_variation_index_selects_and_clamps_model_asset() {
    let model = VisualModel {
        name: "Default".to_owned(),
        component: Some(Component {
            logic: Some(Logic {
                logic_type: "Variation".to_owned(),
                entries: vec![
                    LogicEntry {
                        asset: Some(model_asset("crate_01")),
                        ..LogicEntry::default()
                    },
                    LogicEntry {
                        asset: Some(model_asset("crate_02")),
                        ..LogicEntry::default()
                    },
                ],
            }),
            ..Component::default()
        }),
        ..VisualModel::default()
    };
    let visual = Visual {
        default_model: Some("Default".to_owned()),
        models: vec![model],
        logic: None,
    };
    let model = &visual.models[0];
    assert_eq!(model_asset_path(&visual, model, Some(0)), Some("crate_01"));
    assert_eq!(model_asset_path(&visual, model, Some(1)), Some("crate_02"));
    assert_eq!(model_asset_path(&visual, model, Some(99)), Some("crate_02"));
}

fn model_asset(path: &str) -> Asset {
    Asset {
        asset_type: "Model".to_owned(),
        file: Some(path.to_owned()),
        ..Asset::default()
    }
}
