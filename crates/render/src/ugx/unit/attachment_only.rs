//! Visual graphs whose root contains effects but no UGX model asset.

use super::{
    LoadedUnit, UnitAttachment, UnitAttachmentKind, UnitAttachmentTrigger, UnitLoadError,
    canonical_art_path,
};
use glam::Mat4;
use pipeline::database::hw1::visual::{Attachment, Model as VisualModel};

pub(super) fn load(
    definition: &VisualModel,
    local_transform: Mat4,
) -> Result<LoadedUnit, UnitLoadError> {
    let Some(component) = &definition.component else {
        return Err(UnitLoadError::ModelAssetMissing(definition.name.clone()));
    };
    let mut attachments = component
        .assets
        .iter()
        .filter_map(|asset| direct_asset(definition, asset, local_transform))
        .collect::<Vec<_>>();
    append_authored(
        &mut attachments,
        definition,
        &component.attachments,
        &UnitAttachmentTrigger::Persistent,
        local_transform,
    )?;
    for animation in &definition.anims {
        append_authored(
            &mut attachments,
            definition,
            &animation.attachments,
            &UnitAttachmentTrigger::Animation(animation.anim_type.clone()),
            local_transform,
        )?;
    }
    if attachments.is_empty() {
        return Err(UnitLoadError::ModelAssetMissing(definition.name.clone()));
    }
    Ok(LoadedUnit {
        instances: Vec::new(),
        attachments,
    })
}

fn direct_asset(
    definition: &VisualModel,
    asset: &pipeline::database::hw1::visual::Asset,
    local_transform: Mat4,
) -> Option<UnitAttachment> {
    let kind = asset_kind(&asset.asset_type)?;
    let name = asset.file.as_ref()?.clone();
    Some(UnitAttachment {
        component: definition.name.clone(),
        kind,
        asset_path: Some(canonical_art_path(&name)),
        name,
        to_bone: None,
        from_bone: None,
        sync_animations: false,
        trigger: UnitAttachmentTrigger::Persistent,
        anchor_transform: local_transform,
        target_bone_resolved: true,
    })
}

fn append_authored(
    output: &mut Vec<UnitAttachment>,
    definition: &VisualModel,
    attachments: &[Attachment],
    trigger: &UnitAttachmentTrigger,
    local_transform: Mat4,
) -> Result<(), UnitLoadError> {
    for attachment in attachments {
        let kind = UnitAttachmentKind::from_authored(&attachment.attach_type)?;
        if matches!(
            kind,
            UnitAttachmentKind::ModelReference | UnitAttachmentKind::Model
        ) {
            continue;
        }
        output.push(UnitAttachment {
            component: definition.name.clone(),
            kind,
            name: attachment.name.clone(),
            asset_path: Some(canonical_art_path(&attachment.name)),
            to_bone: attachment.to_bone.clone(),
            from_bone: attachment.from_bone.clone(),
            sync_animations: attachment.sync_anims.unwrap_or(false),
            trigger: trigger.clone(),
            anchor_transform: local_transform,
            target_bone_resolved: attachment.to_bone.is_none(),
        });
    }
    Ok(())
}

fn asset_kind(value: &str) -> Option<UnitAttachmentKind> {
    if value.eq_ignore_ascii_case("Particle") || value.eq_ignore_ascii_case("ParticleFile") {
        Some(UnitAttachmentKind::Particle)
    } else if value.eq_ignore_ascii_case("TerrainEffect") {
        Some(UnitAttachmentKind::TerrainEffect)
    } else if value.eq_ignore_ascii_case("Light") || value.eq_ignore_ascii_case("LightFile") {
        Some(UnitAttachmentKind::Light)
    } else {
        None
    }
}
