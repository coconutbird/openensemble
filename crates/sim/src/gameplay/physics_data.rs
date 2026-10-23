//! The physics data of the proto objects: a body template for every physics info of the
//! library, and the two infos of each proto object.
//!
//! `BDatabase_preloadPhysicsFiles` (`0x1401F2340`) loads every physics file at startup. A proto
//! object names its infos in `<PhysicsInfo>` and `<PhysicsReplacementInfo>`
//! (`BProtoObject_load`, `0x140341620`).

use std::collections::BTreeMap;
use std::sync::Arc;

use pipeline::database::hw1::{Database, ProtoObject};
use pipeline::hw1::physics::PhysicsLibrary;

use super::GameplayCatalog;
use crate::entities::FlightControllerKind;
use crate::physics::{ObjectPhysicsTemplates, PhysicsBlueprintBody, PhysicsTemplate};

mod flight;

pub(crate) use flight::FlightControllerProfile;
use flight::controller_profile as flight_controller_profile;

/// A physics reference of a proto object that makes no body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsLoadIssue {
    proto_object_name: String,
    reference: String,
    reason: String,
}

impl PhysicsLoadIssue {
    /// The proto object.
    #[must_use]
    pub fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    /// The physics info reference, or the element that misses one.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Why the reference makes no body.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct PhysicsCatalog {
    /// The templates by lowercase physics info name.
    infos: BTreeMap<String, Arc<PhysicsTemplate>>,
    /// The blueprint bodies by lowercase blueprint name.
    blueprints: BTreeMap<String, PhysicsBlueprintBody>,
    /// The two infos of each proto object, by lowercase proto name.
    objects: BTreeMap<String, ObjectPhysicsTemplates>,
    flight_controllers: BTreeMap<String, FlightControllerProfile>,
    issues: Vec<PhysicsLoadIssue>,
}

impl PhysicsCatalog {
    /// Builds the templates of every info and blueprint of the library, then joins the proto
    /// objects to them.
    pub(super) fn load(database: &Database, library: &PhysicsLibrary) -> Self {
        let mut catalog = Self::default();
        for name in library.blueprint_names() {
            if let Ok(resolved) = library.resolve_blueprint(name) {
                catalog.blueprints.insert(
                    name.to_ascii_lowercase(),
                    PhysicsBlueprintBody::from_resolved(&resolved),
                );
            }
        }
        let mut failures = BTreeMap::new();
        for name in library.info_names() {
            let key = name.to_ascii_lowercase();
            match library.resolve_info(name) {
                Ok(resolved) => match PhysicsTemplate::from_resolved(&resolved) {
                    Some(template) => {
                        catalog.infos.insert(key, Arc::new(template));
                    }
                    None => {
                        failures.insert(key, "the info names too few blueprints".to_owned());
                    }
                },
                Err(error) => {
                    failures.insert(key, error.to_string());
                }
            }
        }
        for object in &database.objects {
            catalog.add_object(object, &failures);
        }
        catalog
    }

    /// The template of a physics info (any case).
    pub(super) fn info(&self, name: &str) -> Option<&Arc<PhysicsTemplate>> {
        self.infos.get(&name.to_ascii_lowercase())
    }

    fn add_object(&mut self, object: &ProtoObject, failures: &BTreeMap<String, String>) {
        let key = object.name.to_ascii_lowercase();
        let info = self.object_template(object, object.physics_info.as_deref(), failures);
        let replacement =
            self.object_template(object, object.physics_replacement_info.as_deref(), failures);
        if replacement.is_none()
            && reference(object.physics_replacement_info.as_deref()).is_none()
            && has_flag(object, "PhysicsDetonateOnDeath")
        {
            self.issues.push(PhysicsLoadIssue {
                proto_object_name: object.name.clone(),
                reference: "<PhysicsReplacementInfo>".to_owned(),
                reason: "flagged object has no physics replacement reference".to_owned(),
            });
        }
        let has_info_reference = reference(object.physics_info.as_deref()).is_some();
        let vehicle_type = info.as_ref().map(|template| template.vehicle_type());
        if (info.is_some() || !has_info_reference)
            && let Some(profile) = flight_controller_profile(object, vehicle_type)
        {
            self.flight_controllers.insert(key.clone(), profile);
        }
        if info.is_some() || replacement.is_some() {
            self.objects
                .insert(key, ObjectPhysicsTemplates { info, replacement });
        }
    }

    fn object_template(
        &mut self,
        object: &ProtoObject,
        name: Option<&str>,
        failures: &BTreeMap<String, String>,
    ) -> Option<Arc<PhysicsTemplate>> {
        let name = reference(name)?;
        let key = name.to_ascii_lowercase();
        if let Some(template) = self.infos.get(&key) {
            return Some(Arc::clone(template));
        }
        self.issues.push(PhysicsLoadIssue {
            proto_object_name: object.name.clone(),
            reference: name.to_owned(),
            reason: failures
                .get(&key)
                .cloned()
                .unwrap_or_else(|| "the physics info is not in the library".to_owned()),
        });
        None
    }
}

impl GameplayCatalog {
    /// The two physics infos of a proto object (any case).
    #[must_use]
    pub fn object_physics(&self, proto_object_name: &str) -> Option<&ObjectPhysicsTemplates> {
        self.physics
            .objects
            .get(&proto_object_name.to_ascii_lowercase())
    }

    /// The template of a physics info (any case).
    #[must_use]
    pub fn physics_info(&self, name: &str) -> Option<&Arc<PhysicsTemplate>> {
        self.physics.infos.get(&name.to_ascii_lowercase())
    }

    /// The `<PhysicsReplacementInfo>` template of a proto object (any case).
    #[must_use]
    pub fn physics_replacement(&self, proto_object_name: &str) -> Option<&Arc<PhysicsTemplate>> {
        self.object_physics(proto_object_name)?.replacement.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn insert_test_physics_replacement(
        &mut self,
        proto_object_name: &str,
        template: Arc<PhysicsTemplate>,
    ) {
        self.physics
            .objects
            .entry(proto_object_name.to_ascii_lowercase())
            .or_default()
            .replacement = Some(template);
    }

    /// The body of a blueprint (any case), for code-built bodies such as debris parts.
    #[must_use]
    pub fn physics_blueprint(&self, name: &str) -> Option<&PhysicsBlueprintBody> {
        self.physics.blueprints.get(&name.to_ascii_lowercase())
    }

    /// The physics references that make no body, in object order.
    #[must_use]
    pub fn physics_issues(&self) -> &[PhysicsLoadIssue] {
        &self.physics.issues
    }

    /// Return the retail flying movement action selected for a layered prototype.
    #[must_use]
    pub fn flight_controller_kind(
        &self,
        proto_object_name: &str,
        enable_flight: bool,
    ) -> FlightControllerKind {
        self.physics
            .flight_controllers
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(FlightControllerKind::Direct, |profile| {
                profile.resolve(enable_flight)
            })
    }

    /// Sets the physics infos of a proto object, for tests and tools.
    pub fn insert_object_physics(
        &mut self,
        proto_object_name: &str,
        templates: ObjectPhysicsTemplates,
    ) {
        self.physics
            .objects
            .insert(proto_object_name.to_ascii_lowercase(), templates);
    }

    pub(crate) fn object_physics_entries(
        &self,
    ) -> impl Iterator<Item = (&str, &ObjectPhysicsTemplates)> + '_ {
        self.physics
            .objects
            .iter()
            .map(|(name, templates)| (name.as_str(), templates))
    }

    pub(crate) fn flight_controller_profiles(
        &self,
    ) -> impl Iterator<Item = (&str, FlightControllerProfile)> + '_ {
        self.physics
            .flight_controllers
            .iter()
            .map(|(name, profile)| (name.as_str(), *profile))
    }
}

/// A physics reference without space, or `None` for an empty one.
fn reference(name: Option<&str>) -> Option<&str> {
    name.map(str::trim).filter(|name| !name.is_empty())
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}
