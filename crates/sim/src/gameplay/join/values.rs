//! Scenario-layered database values consumed by retail Join actions.

use pipeline::database::hw1::tactics::ProtoObjectRef;
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct JoinAttachmentProfile {
    proto_object_id: i32,
    proto_object_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct JoinDatabaseProfiles {
    attachment_objects: BTreeMap<String, JoinAttachmentProfile>,
}

impl JoinAttachmentProfile {
    pub(crate) const fn proto_object_id(&self) -> i32 {
        self.proto_object_id
    }

    pub(crate) fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }
}

impl JoinDatabaseProfiles {
    pub(crate) fn from_database(database: &Database) -> Self {
        let attachment_objects = database
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| is_attachment_object(object))
            .map(|(index, object)| {
                (
                    object.name.to_ascii_lowercase(),
                    JoinAttachmentProfile {
                        proto_object_id: database_id(object, index),
                        proto_object_name: object.name.clone(),
                    },
                )
            })
            .collect();
        Self { attachment_objects }
    }

    pub(crate) fn resolve_attachment(
        &self,
        reference: Option<&ProtoObjectRef>,
    ) -> Option<&JoinAttachmentProfile> {
        let reference = reference?;
        if reference.squad.is_some() {
            return None;
        }
        self.attachment_objects
            .get(&reference.name.trim().to_ascii_lowercase())
    }
}

fn is_attachment_object(object: &ProtoObject) -> bool {
    object
        .object_class
        .as_deref()
        .is_none_or(|class| class.trim().eq_ignore_ascii_case("Object"))
}

fn database_id(object: &ProtoObject, index: usize) -> i32 {
    object
        .dbid
        .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_object_class_uses_retail_class_zero_attachment_default() {
        let database = Database {
            objects: vec![ProtoObject {
                name: "fx_hijacked".to_owned(),
                dbid: Some(3883),
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let action = crate::gameplay::join::JoinActionProfile::from_action(
            &pipeline::database::hw1::tactics::Action {
                proto_object: Some(ProtoObjectRef {
                    name: "FX_HIJACKED".to_owned(),
                    ..ProtoObjectRef::default()
                }),
                ..Default::default()
            },
        );
        let profiles = JoinDatabaseProfiles::from_database(&database);
        let attachment = profiles
            .resolve_attachment(action.attachment())
            .expect("an omitted ObjectClass retains retail's class-zero default");

        assert_eq!(attachment.proto_object_id(), 3883);
        assert_eq!(attachment.proto_object_name(), "fx_hijacked");
    }
}
