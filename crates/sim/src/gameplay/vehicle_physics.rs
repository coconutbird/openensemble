//! Scenario-layered rigid-body definitions for supported ground vehicles.
//!
//! `WorldLoadOptions::runtime()` deliberately leaves physics chains unresolved.
//! The simulation still needs those immutable values, so this catalog follows
//! each proto object's `PhysicsInfo` reference through `.physics`, `.blueprint`,
//! and `.shp` using the same already-layered asset source as tactics.

use super::GameplayCatalog;
use crate::entities::FlightControllerKind;
use crate::physics::{BoxCollider, PhysicsMaterial};
use crate::sync::SyncChecksum;
use glam::Vec3;
use pipeline::database::hw1::physics::{
    Blueprint, Clamshell, ClamshellPart, PhysicsVehicleType, Shape, parse_blueprint, parse_physics,
    parse_shape,
};
use pipeline::database::hw1::{Database, ProtoObject, Vector3};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::BTreeMap;

mod flight;

pub(crate) use flight::FlightControllerProfile;
use flight::controller_profile as flight_controller_profile;

/// Ground-vehicle controller selected by a retail `.physics` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundVehicleKind {
    /// Wheeled Warthog controller.
    Warthog,
    /// Hovering Ghost controller.
    Ghost,
}

/// Immutable material and collider data resolved for one proto object.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundVehiclePhysicsProfile {
    physics_info: String,
    kind: GroundVehicleKind,
    material: PhysicsMaterial,
    collider: BoxCollider,
}

/// Immutable body used when a unit creates its physics replacement.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsReplacementProfile {
    physics_info: String,
    material: PhysicsMaterial,
    collider: BoxCollider,
    clamshell: bool,
}

/// A referenced vehicle physics chain that could not be loaded completely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehiclePhysicsLoadIssue {
    proto_object_name: String,
    asset_path: String,
    reason: String,
}

/// A referenced physics-replacement chain that could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsReplacementLoadIssue {
    proto_object_name: String,
    asset_path: String,
    reason: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct GroundVehiclePhysicsCatalog {
    profiles: BTreeMap<String, GroundVehiclePhysicsProfile>,
    flight_controllers: BTreeMap<String, FlightControllerProfile>,
    issues: Vec<VehiclePhysicsLoadIssue>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct PhysicsReplacementCatalog {
    profiles: BTreeMap<String, PhysicsReplacementProfile>,
    issues: Vec<PhysicsReplacementLoadIssue>,
}

#[derive(Debug, Clone)]
struct AssetFailure {
    path: String,
    reason: String,
}

#[derive(Debug, Clone)]
struct VehiclePhysicsAsset {
    vehicle_type: PhysicsVehicleType,
    ground_profile: Option<GroundVehiclePhysicsProfile>,
}

#[derive(Debug)]
struct ReplacementBody {
    material_blueprint: Blueprint,
    half_extents: Vec3,
    local_center: Vec3,
    clamshell: bool,
}

impl GroundVehiclePhysicsCatalog {
    pub(super) fn load(database: &Database, source: &mut AssetSource<StdFileProvider>) -> Self {
        let mut result = Self::default();
        let mut cache = BTreeMap::<String, Result<VehiclePhysicsAsset, AssetFailure>>::new();
        for object in &database.objects {
            let reference = object
                .physics_info
                .as_deref()
                .map(str::trim)
                .filter(|reference| !reference.is_empty());
            let Some(reference) = reference else {
                if let Some(profile) = flight_controller_profile(object, None) {
                    result
                        .flight_controllers
                        .insert(object.name.to_ascii_lowercase(), profile);
                }
                continue;
            };
            let key = reference.to_ascii_lowercase();
            if !cache.contains_key(&key) {
                cache.insert(key.clone(), load_vehicle_physics_asset(reference, source));
            }
            match cache.get(&key).expect("physics cache entry") {
                Ok(asset) => {
                    let object_key = object.name.to_ascii_lowercase();
                    if let Some(profile) = &asset.ground_profile {
                        result.profiles.insert(object_key.clone(), profile.clone());
                    }
                    if let Some(profile) =
                        flight_controller_profile(object, Some(asset.vehicle_type))
                    {
                        result.flight_controllers.insert(object_key, profile);
                    }
                }
                Err(failure) => result.issues.push(VehiclePhysicsLoadIssue {
                    proto_object_name: object.name.clone(),
                    asset_path: failure.path.clone(),
                    reason: failure.reason.clone(),
                }),
            }
        }
        result
    }
}

impl PhysicsReplacementCatalog {
    pub(super) fn load(database: &Database, source: &mut AssetSource<StdFileProvider>) -> Self {
        let mut result = Self::default();
        let mut cache = BTreeMap::<String, Result<PhysicsReplacementProfile, AssetFailure>>::new();
        for object in &database.objects {
            let requires_replacement = has_proto_flag(object, "PhysicsDetonateOnDeath");
            let Some(reference) = object
                .physics_replacement_info
                .as_deref()
                .map(str::trim)
                .filter(|reference| !reference.is_empty())
            else {
                if requires_replacement {
                    result.issues.push(PhysicsReplacementLoadIssue {
                        proto_object_name: object.name.clone(),
                        asset_path: "<PhysicsReplacementInfo>".to_owned(),
                        reason: "flagged object has no physics replacement reference".to_owned(),
                    });
                }
                continue;
            };
            let key = reference.to_ascii_lowercase();
            if !cache.contains_key(&key) {
                cache.insert(key.clone(), load_replacement_profile(reference, source));
            }
            match cache.get(&key).expect("replacement physics cache entry") {
                Ok(profile) => {
                    result
                        .profiles
                        .insert(object.name.to_ascii_lowercase(), profile.clone());
                }
                Err(failure) => result.issues.push(PhysicsReplacementLoadIssue {
                    proto_object_name: object.name.clone(),
                    asset_path: failure.path.clone(),
                    reason: failure.reason.clone(),
                }),
            }
        }
        result
    }

    #[cfg(test)]
    pub(super) fn insert(&mut self, proto_object_name: &str, profile: PhysicsReplacementProfile) {
        self.profiles
            .insert(proto_object_name.to_ascii_lowercase(), profile);
    }
}

impl GameplayCatalog {
    /// Look up scenario-layered ground-vehicle physics by proto-object name.
    #[must_use]
    pub fn ground_vehicle_physics(
        &self,
        proto_object_name: &str,
    ) -> Option<&GroundVehiclePhysicsProfile> {
        self.vehicle_physics
            .profiles
            .get(&proto_object_name.to_ascii_lowercase())
    }

    /// Return deterministic vehicle-physics load diagnostics in object order.
    #[must_use]
    pub fn vehicle_physics_issues(&self) -> &[VehiclePhysicsLoadIssue] {
        &self.vehicle_physics.issues
    }

    /// Return the retail flying movement action selected for a layered prototype.
    #[must_use]
    pub fn flight_controller_kind(
        &self,
        proto_object_name: &str,
        enable_flight: bool,
    ) -> FlightControllerKind {
        self.vehicle_physics
            .flight_controllers
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(FlightControllerKind::Direct, |profile| {
                profile.resolve(enable_flight)
            })
    }

    /// Look up the layered physics-replacement body for one prototype.
    #[must_use]
    pub fn physics_replacement(
        &self,
        proto_object_name: &str,
    ) -> Option<&PhysicsReplacementProfile> {
        self.physics_replacements
            .profiles
            .get(&proto_object_name.to_ascii_lowercase())
    }

    /// Return deterministic physics-replacement load diagnostics.
    #[must_use]
    pub fn physics_replacement_issues(&self) -> &[PhysicsReplacementLoadIssue] {
        &self.physics_replacements.issues
    }

    #[cfg(test)]
    pub(crate) fn insert_test_physics_replacement(
        &mut self,
        proto_object_name: &str,
        profile: PhysicsReplacementProfile,
    ) {
        self.physics_replacements.insert(proto_object_name, profile);
    }

    pub(crate) fn ground_vehicle_physics_profiles(
        &self,
    ) -> impl Iterator<Item = (&str, &GroundVehiclePhysicsProfile)> + '_ {
        self.vehicle_physics
            .profiles
            .iter()
            .map(|(name, profile)| (name.as_str(), profile))
    }

    pub(crate) fn flight_controller_profiles(
        &self,
    ) -> impl Iterator<Item = (&str, FlightControllerProfile)> + '_ {
        self.vehicle_physics
            .flight_controllers
            .iter()
            .map(|(name, profile)| (name.as_str(), *profile))
    }
}

impl GroundVehiclePhysicsProfile {
    /// Return the original `PhysicsInfo` key.
    #[must_use]
    pub fn physics_info(&self) -> &str {
        &self.physics_info
    }

    /// Return the retail vehicle controller class.
    #[must_use]
    pub const fn kind(&self) -> GroundVehicleKind {
        self.kind
    }

    /// Return sanitized blueprint material properties.
    #[must_use]
    pub const fn material(&self) -> PhysicsMaterial {
        self.material
    }

    /// Return the box resolved from the Havok shape and physics center offset.
    #[must_use]
    pub const fn collider(&self) -> BoxCollider {
        self.collider
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(match self.kind {
            GroundVehicleKind::Warthog => 0,
            GroundVehicleKind::Ghost => 1,
        });
        hash_string(checksum, &self.physics_info);
        checksum.hash_f32(self.material.mass);
        checksum.hash_f32(self.material.friction);
        checksum.hash_f32(self.material.restitution);
        checksum.hash_f32(self.material.linear_damping);
        checksum.hash_f32(self.material.angular_damping);
        checksum.hash_vec3(
            self.collider.half_extents.x,
            self.collider.half_extents.y,
            self.collider.half_extents.z,
        );
        checksum.hash_vec3(
            self.collider.center_offset.x,
            self.collider.center_offset.y,
            self.collider.center_offset.z,
        );
    }
}

impl PhysicsReplacementProfile {
    /// Build a sanitized replacement profile for deterministic fixtures.
    #[must_use]
    pub fn new(
        physics_info: impl Into<String>,
        material: PhysicsMaterial,
        collider: BoxCollider,
    ) -> Self {
        Self {
            physics_info: physics_info.into(),
            material: sanitize_profile_material(material),
            collider,
            clamshell: false,
        }
    }

    /// Mark a deterministic fixture as a retail multi-body clamshell.
    #[must_use]
    pub const fn with_clamshell(mut self) -> Self {
        self.clamshell = true;
        self
    }

    /// Return the original `PhysicsReplacementInfo` key.
    #[must_use]
    pub fn physics_info(&self) -> &str {
        &self.physics_info
    }

    /// Return sanitized blueprint material properties.
    #[must_use]
    pub const fn material(&self) -> PhysicsMaterial {
        self.material
    }

    /// Return the replacement collision box and center offset.
    #[must_use]
    pub const fn collider(&self) -> BoxCollider {
        self.collider
    }

    /// Return whether the replacement originated from a clamshell definition.
    #[must_use]
    pub const fn is_clamshell(&self) -> bool {
        self.clamshell
    }
}

impl VehiclePhysicsLoadIssue {
    /// Return the proto object whose physics chain failed.
    #[must_use]
    pub fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    /// Return the canonical asset path at which loading failed.
    #[must_use]
    pub fn asset_path(&self) -> &str {
        &self.asset_path
    }

    /// Return the parse or resolution failure.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl PhysicsReplacementLoadIssue {
    /// Return the prototype whose replacement failed to load.
    #[must_use]
    pub fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    /// Return the canonical asset path at which loading failed.
    #[must_use]
    pub fn asset_path(&self) -> &str {
        &self.asset_path
    }

    /// Return the parse or resolution failure.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

fn load_vehicle_physics_asset(
    physics_info: &str,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<VehiclePhysicsAsset, AssetFailure> {
    let physics_path = canonical_physics_path(physics_info, "physics");
    let document = read_document(source, &physics_path)?;
    let physics = parse_physics(&document).map_err(|error| AssetFailure {
        path: physics_path.clone(),
        reason: format!("failed to parse physics data: {error}"),
    })?;
    let vehicle_type = physics
        .vehicle
        .as_ref()
        .map_or(PhysicsVehicleType::None, |vehicle| vehicle.vehicle_type);
    let Some(kind) = supported_kind(vehicle_type) else {
        return Ok(VehiclePhysicsAsset {
            vehicle_type,
            ground_profile: None,
        });
    };
    let blueprint_ref = physics
        .primary_blueprint()
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
        .ok_or_else(|| AssetFailure {
            path: physics_path,
            reason: "supported vehicle has no primary blueprint".to_owned(),
        })?;
    let blueprint_path = canonical_physics_path(blueprint_ref, "blueprint");
    let blueprint_document = read_document(source, &blueprint_path)?;
    let blueprint = parse_blueprint(&blueprint_document).map_err(|error| AssetFailure {
        path: blueprint_path,
        reason: format!("failed to parse blueprint data: {error}"),
    })?;
    let half_extents = load_half_extents(source, &blueprint)?;
    Ok(VehiclePhysicsAsset {
        vehicle_type,
        ground_profile: Some(GroundVehiclePhysicsProfile {
            physics_info: physics_info.trim().to_owned(),
            kind,
            material: material_from_blueprint(&blueprint),
            collider: BoxCollider::new(half_extents, vector_or_zero(physics.center_offset)),
        }),
    })
}

fn load_replacement_profile(
    physics_info: &str,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<PhysicsReplacementProfile, AssetFailure> {
    let physics_path = canonical_physics_path(physics_info, "physics");
    let document = read_document(source, &physics_path)?;
    let physics = parse_physics(&document).map_err(|error| AssetFailure {
        path: physics_path.clone(),
        reason: format!("failed to parse replacement physics data: {error}"),
    })?;
    let body = resolve_replacement_body(&physics, source).map_err(|mut failure| {
        if failure.path == "<replacement physics>" {
            failure.path = physics_path;
        }
        failure
    })?;
    Ok(PhysicsReplacementProfile {
        physics_info: physics_info.trim().to_owned(),
        material: material_from_blueprint(&body.material_blueprint),
        collider: BoxCollider::new(
            body.half_extents,
            vector_or_zero(physics.center_offset) + body.local_center,
        ),
        clamshell: body.clamshell,
    })
}

pub(super) fn load_dynamic_body_profile(
    physics_info: &str,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<PhysicsReplacementProfile, String> {
    load_replacement_profile(physics_info, source)
        .map_err(|failure| format!("{}: {}", failure.path, failure.reason))
}

fn resolve_replacement_body(
    physics: &pipeline::database::hw1::physics::Physics,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<ReplacementBody, AssetFailure> {
    if let Some(clamshell) = &physics.clamshell {
        return resolve_clamshell_body(clamshell, source);
    }
    let reference = physics
        .primary_blueprint()
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
        .ok_or_else(|| AssetFailure {
            path: "<replacement physics>".to_owned(),
            reason: "replacement physics has no primary blueprint".to_owned(),
        })?;
    let (material_blueprint, half_extents) = load_replacement_blueprint(reference, source)?;
    Ok(ReplacementBody {
        material_blueprint,
        half_extents,
        local_center: Vec3::ZERO,
        clamshell: false,
    })
}

fn resolve_clamshell_body(
    clamshell: &Clamshell,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<ReplacementBody, AssetFailure> {
    let parts = [
        clamshell.upper.as_ref(),
        clamshell.lower.as_ref(),
        clamshell.pelvis.as_ref(),
    ];
    let mut material_blueprint = None;
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for part in parts.into_iter().flatten() {
        let (blueprint, half_extents) = load_clamshell_part(part, source)?;
        let center = Vec3::Y * part.height_offset.unwrap_or_default();
        minimum = minimum.min(center - half_extents);
        maximum = maximum.max(center + half_extents);
        if material_blueprint.is_none() {
            material_blueprint = Some(blueprint);
        }
    }
    let material_blueprint = material_blueprint.ok_or_else(|| AssetFailure {
        path: "<replacement physics>".to_owned(),
        reason: "replacement clamshell has no body blueprints".to_owned(),
    })?;
    let local_center = (minimum + maximum) * 0.5;
    let half_extents = (maximum - minimum) * 0.5;
    if !local_center.is_finite() || !valid_half_extents(half_extents) {
        return Err(AssetFailure {
            path: "<replacement physics>".to_owned(),
            reason: "replacement clamshell has invalid aggregate bounds".to_owned(),
        });
    }
    Ok(ReplacementBody {
        material_blueprint,
        half_extents,
        local_center,
        clamshell: true,
    })
}

fn load_clamshell_part(
    part: &ClamshellPart,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<(Blueprint, Vec3), AssetFailure> {
    if part.height_offset.is_some_and(|offset| !offset.is_finite()) {
        return Err(AssetFailure {
            path: "<replacement physics>".to_owned(),
            reason: "replacement clamshell has a non-finite height offset".to_owned(),
        });
    }
    load_replacement_blueprint(&part.blueprint, source)
}

fn load_replacement_blueprint(
    reference: &str,
    source: &mut AssetSource<StdFileProvider>,
) -> Result<(Blueprint, Vec3), AssetFailure> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(AssetFailure {
            path: "<replacement physics>".to_owned(),
            reason: "replacement body has an empty blueprint reference".to_owned(),
        });
    }
    let path = canonical_physics_path(reference, "blueprint");
    let document = read_document(source, &path)?;
    let blueprint = parse_blueprint(&document).map_err(|error| AssetFailure {
        path,
        reason: format!("failed to parse replacement blueprint data: {error}"),
    })?;
    let half_extents = load_half_extents(source, &blueprint)?;
    Ok((blueprint, half_extents))
}

fn load_half_extents(
    source: &mut AssetSource<StdFileProvider>,
    blueprint: &Blueprint,
) -> Result<Vec3, AssetFailure> {
    if let Some(shape_ref) = blueprint
        .shape
        .as_deref()
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        let path = canonical_physics_path(shape_ref, "shp");
        let document = read_document(source, &path)?;
        let shape = parse_shape(&document).map_err(|error| AssetFailure {
            path: path.clone(),
            reason: format!("failed to parse shape data: {error}"),
        })?;
        return shape_half_extents(&shape).ok_or_else(|| AssetFailure {
            path,
            reason: "shape has no supported positive collision bounds".to_owned(),
        });
    }
    blueprint
        .half_extents
        .and_then(valid_vector)
        .ok_or_else(|| AssetFailure {
            path: "<blueprint halfExtents>".to_owned(),
            reason: "blueprint has no shape or valid halfExtents".to_owned(),
        })
}

fn read_document(
    source: &mut AssetSource<StdFileProvider>,
    path: &str,
) -> Result<pipeline::xmb::Document, AssetFailure> {
    source.read_xmb(path).ok_or_else(|| AssetFailure {
        path: path.to_owned(),
        reason: "asset was not found or was not a valid XMB document".to_owned(),
    })
}

pub(super) fn load_blueprint_material(
    source: &mut AssetSource<StdFileProvider>,
    reference: &str,
) -> Result<PhysicsMaterial, String> {
    let path = canonical_physics_path(reference, "blueprint");
    let document = source
        .read_xmb(&path)
        .ok_or_else(|| format!("physics blueprint {path} was not found or valid XMB"))?;
    let blueprint = parse_blueprint(&document)
        .map_err(|error| format!("failed to parse physics blueprint {path}: {error}"))?;
    Ok(material_from_blueprint(&blueprint))
}

fn supported_kind(vehicle_type: PhysicsVehicleType) -> Option<GroundVehicleKind> {
    match vehicle_type {
        PhysicsVehicleType::Warthog => Some(GroundVehicleKind::Warthog),
        PhysicsVehicleType::Ghost => Some(GroundVehicleKind::Ghost),
        _ => None,
    }
}

fn material_from_blueprint(blueprint: &Blueprint) -> PhysicsMaterial {
    let default = PhysicsMaterial::default();
    PhysicsMaterial {
        mass: positive_or(blueprint.mass, default.mass),
        friction: nonnegative_or(blueprint.friction, default.friction),
        restitution: finite_or(blueprint.restitution, default.restitution).clamp(0.0, 1.0),
        linear_damping: nonnegative_or(blueprint.linear_damping, default.linear_damping),
        angular_damping: nonnegative_or(blueprint.angular_damping, default.angular_damping),
    }
}

fn sanitize_profile_material(material: PhysicsMaterial) -> PhysicsMaterial {
    let default = PhysicsMaterial::default();
    PhysicsMaterial {
        mass: if material.mass.is_finite() && material.mass > 0.0 {
            material.mass
        } else {
            default.mass
        },
        friction: if material.friction.is_finite() && material.friction >= 0.0 {
            material.friction
        } else {
            default.friction
        },
        restitution: finite_or(Some(material.restitution), default.restitution).clamp(0.0, 1.0),
        linear_damping: nonnegative_or(Some(material.linear_damping), default.linear_damping),
        angular_damping: nonnegative_or(Some(material.angular_damping), default.angular_damping),
    }
}

fn has_proto_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn box_half_extents(shape: &Shape) -> Option<Vec3> {
    shape
        .objects
        .iter()
        .filter(|object| object.object_type.eq_ignore_ascii_case("hkBoxShape"))
        .flat_map(|object| &object.params)
        .find(|parameter| parameter.name.eq_ignore_ascii_case("halfExtents"))
        .and_then(|parameter| parse_havok_vector(&parameter.value))
        .filter(|value| valid_half_extents(*value))
}

fn shape_half_extents(shape: &Shape) -> Option<Vec3> {
    box_half_extents(shape).or_else(|| sphere_half_extents(shape))
}

fn sphere_half_extents(shape: &Shape) -> Option<Vec3> {
    shape
        .objects
        .iter()
        .filter(|object| object.object_type.eq_ignore_ascii_case("hkSphereShape"))
        .flat_map(|object| &object.params)
        .find(|parameter| parameter.name.eq_ignore_ascii_case("radius"))
        .and_then(|parameter| parameter.value.trim().parse::<f32>().ok())
        .filter(|radius| radius.is_finite() && *radius > 0.0)
        .map(Vec3::splat)
}

fn parse_havok_vector(value: &str) -> Option<Vec3> {
    let mut values = value
        .split(|character: char| {
            character == ','
                || character.is_ascii_whitespace()
                || matches!(character, '(' | ')' | '[' | ']' | '{' | '}')
        })
        .filter(|component| !component.is_empty())
        .map(str::parse::<f32>);
    let result = Vec3::new(
        values.next()?.ok()?,
        values.next()?.ok()?,
        values.next()?.ok()?,
    );
    result.is_finite().then_some(result)
}

fn canonical_physics_path(reference: &str, extension: &str) -> String {
    let mut path = reference.trim().replace('/', "\\");
    path = path.trim_start_matches('\\').to_owned();
    if path.to_ascii_lowercase().ends_with(".xmb") {
        path.truncate(path.len().saturating_sub(4));
    }
    if !path.to_ascii_lowercase().starts_with("physics\\") {
        path = format!("physics\\{path}");
    }
    if !path
        .to_ascii_lowercase()
        .ends_with(&format!(".{extension}"))
    {
        path.push('.');
        path.push_str(extension);
    }
    path
}

fn vector_or_zero(value: Option<Vector3>) -> Vec3 {
    value.and_then(finite_vector).unwrap_or(Vec3::ZERO)
}

fn valid_vector(value: Vector3) -> Option<Vec3> {
    finite_vector(value).filter(|value| valid_half_extents(*value))
}

fn finite_vector(value: Vector3) -> Option<Vec3> {
    let value = Vec3::new(value.x, value.y, value.z);
    value.is_finite().then_some(value)
}

fn valid_half_extents(value: Vec3) -> bool {
    value.is_finite() && value.x > 0.0 && value.y > 0.0 && value.z > 0.0
}

fn finite_or(value: Option<f32>, default: f32) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(default)
}

fn nonnegative_or(value: Option<f32>, default: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(default)
}

fn positive_or(value: Option<f32>, default: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(default)
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::physics::{HavokObject, HavokParam};

    #[test]
    fn canonical_paths_accept_bare_and_complete_references() {
        assert_eq!(
            canonical_physics_path("ghost", "physics"),
            "physics\\ghost.physics"
        );
        assert_eq!(
            canonical_physics_path("Physics/ghost.physics.xmb", "physics"),
            "Physics\\ghost.physics"
        );
    }

    #[test]
    fn parses_retail_havok_box_vector() {
        assert_eq!(
            parse_havok_vector("(1.5 1.0 3.0)"),
            Some(Vec3::new(1.5, 1.0, 3.0))
        );
        assert_eq!(
            parse_havok_vector("(1.5, 1.0, 3.0, 0.0)"),
            Some(Vec3::new(1.5, 1.0, 3.0))
        );
        assert_eq!(parse_havok_vector("missing"), None);
    }

    #[test]
    fn approximates_retail_havok_sphere_as_box_bounds() {
        let shape = Shape {
            objects: vec![HavokObject {
                object_type: "hkSphereShape".to_owned(),
                params: vec![HavokParam {
                    name: "radius".to_owned(),
                    value: "0.1500".to_owned(),
                    ..HavokParam::default()
                }],
                ..HavokObject::default()
            }],
            ..Shape::default()
        };

        assert_eq!(shape_half_extents(&shape), Some(Vec3::splat(0.15)));
    }
}
