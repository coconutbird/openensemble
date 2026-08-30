//! Scenario-layered damage-template parts used by retail throw-part actions.

use super::{GameplayCatalog, vehicle_physics};
use crate::physics::{BoxCollider, PhysicsMaterial};
use glam::Vec3;
use pipeline::database::hw1::visual::{Asset, Visual};
use pipeline::database::hw1::{Database, ProtoObject, visual};
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::ugx::{GrannyMesh, Reader, UgxGeom};
use pipeline::xmb::{Document, Node};
use std::collections::BTreeMap;

const PART_BOUNDING_BOX_FACTOR: f32 = 0.75;

/// Throw-part choices in authored impact-point order for one visual model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamagePartProfile {
    impact_points: Vec<Option<ThrownDamagePart>>,
}

/// One valid `throwpart` action resolved against its visual model and blueprint.
#[derive(Debug, Clone, PartialEq)]
pub struct ThrownDamagePart {
    mesh_names: Vec<String>,
    collider: BoxCollider,
    material: PhysicsMaterial,
    force_multiplier: f32,
    throw_center_offset: Vec3,
    single_mesh: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DamagePartCatalog {
    profiles: BTreeMap<String, DamagePartProfile>,
}

#[derive(Default)]
struct AssetCache {
    visuals: BTreeMap<String, Option<Visual>>,
    damage_templates: BTreeMap<String, Option<Document>>,
    geometry: BTreeMap<String, Option<UgxGeom>>,
    materials: BTreeMap<String, Option<PhysicsMaterial>>,
}

struct ProfileAssets {
    model_path: String,
    document: Document,
}

impl DamagePartCatalog {
    pub(super) fn load(database: &Database, source: &mut AssetSource<StdFileProvider>) -> Self {
        let mut profiles = BTreeMap::new();
        let mut cache = AssetCache::default();
        for object in &database.objects {
            if let Some(profile) = load_object_profile(object, source, &mut cache) {
                profiles.insert(object.name.to_ascii_lowercase(), profile);
            }
        }
        Self { profiles }
    }

    #[cfg(test)]
    fn insert(&mut self, proto_object_name: &str, profile: DamagePartProfile) {
        self.profiles
            .insert(proto_object_name.to_ascii_lowercase(), profile);
    }
}

impl GameplayCatalog {
    /// Return scenario-layered throw-part data for one prototype's default visual.
    #[must_use]
    pub fn damage_parts(&self, proto_object_name: &str) -> Option<&DamagePartProfile> {
        self.damage_parts
            .profiles
            .get(&proto_object_name.to_ascii_lowercase())
    }

    #[cfg(test)]
    pub(crate) fn insert_test_damage_parts(
        &mut self,
        proto_object_name: &str,
        profile: DamagePartProfile,
    ) {
        self.damage_parts.insert(proto_object_name, profile);
    }
}

impl DamagePartProfile {
    /// Construct deterministic impact-point choices for a fixture or custom catalog.
    #[must_use]
    pub fn new(impact_points: Vec<Option<ThrownDamagePart>>) -> Self {
        Self { impact_points }
    }

    /// Return the number of impact points loaded by the retail damage template.
    #[must_use]
    pub fn impact_point_count(&self) -> usize {
        self.impact_points.len()
    }

    /// Return the final valid throw-part action for one authored impact point.
    #[must_use]
    pub fn impact_point(&self, index: usize) -> Option<&ThrownDamagePart> {
        self.impact_points.get(index).and_then(Option::as_ref)
    }
}

impl ThrownDamagePart {
    /// Construct a resolved throw-part action for deterministic fixtures.
    #[must_use]
    pub fn new(
        mesh_names: Vec<String>,
        collider: BoxCollider,
        material: PhysicsMaterial,
        force_multiplier: f32,
    ) -> Option<Self> {
        let mesh_names = normalized_mesh_names(mesh_names);
        if mesh_names.is_empty() || !valid_force_multiplier(force_multiplier) {
            return None;
        }
        Some(Self {
            single_mesh: mesh_names.len() == 1,
            mesh_names,
            collider,
            material,
            force_multiplier,
            throw_center_offset: collider.center_offset,
        })
    }

    /// Return the source-model meshes rendered by the detached object.
    #[must_use]
    pub fn mesh_names(&self) -> &[String] {
        &self.mesh_names
    }

    /// Return the deterministic approximation of retail's Havok part shape.
    #[must_use]
    pub const fn collider(&self) -> BoxCollider {
        self.collider
    }

    /// Return the scenario-layered physics blueprint material.
    #[must_use]
    pub const fn material(&self) -> PhysicsMaterial {
        self.material
    }

    /// Return the authored impulse-force multiplier.
    #[must_use]
    pub const fn force_multiplier(&self) -> f32 {
        self.force_multiplier
    }

    /// Return retail's averaged model-space center used for force direction.
    #[must_use]
    pub const fn throw_center_offset(&self) -> Vec3 {
        self.throw_center_offset
    }

    /// Return whether retail creates a single box rather than a compound shape.
    #[must_use]
    pub const fn is_single_mesh(&self) -> bool {
        self.single_mesh
    }
}

fn load_object_profile(
    object: &ProtoObject,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<DamagePartProfile> {
    let visual = load_visual(object.visual.as_deref()?, source, cache)?;
    let asset = default_model_asset(&visual)?;
    let assets = load_profile_assets(asset, source, cache)?;
    let impact_nodes = impact_point_nodes(&assets.document);
    if impact_nodes.is_empty() {
        return Some(DamagePartProfile::default());
    }
    let geometry = load_geometry(&assets.model_path, source, cache)?;
    let impact_points = impact_nodes
        .into_iter()
        .filter(|impact| valid_impact_point(impact, &geometry))
        .map(|impact| parse_final_throw_part(impact, &geometry, source, cache))
        .collect();
    Some(DamagePartProfile { impact_points })
}

fn load_visual(
    reference: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<Visual> {
    let path = canonical_art_path(reference, None);
    let key = path.to_ascii_lowercase();
    if !cache.visuals.contains_key(&key) {
        let parsed = source
            .read_xmb(&path)
            .and_then(|document| visual::parse(&document).ok());
        cache.visuals.insert(key.clone(), parsed);
    }
    cache.visuals.get(&key).cloned().flatten()
}

fn default_model_asset(visual: &Visual) -> Option<&Asset> {
    let default_name = visual.default_model.as_deref()?;
    let model = visual
        .models
        .iter()
        .find(|model| model.name.eq_ignore_ascii_case(default_name))?;
    let component = model.component.as_ref()?;
    component
        .logic
        .as_ref()
        .filter(|logic| logic.logic_type.eq_ignore_ascii_case("Variation"))
        .and_then(|logic| logic.entries.first())
        .and_then(|entry| entry.asset.as_ref())
        .filter(|asset| asset.asset_type.eq_ignore_ascii_case("Model"))
        .or_else(|| direct_model_asset(&component.assets))
}

fn direct_model_asset(assets: &[Asset]) -> Option<&Asset> {
    assets
        .iter()
        .find(|asset| asset.asset_type.eq_ignore_ascii_case("Model"))
}

fn load_profile_assets(
    asset: &Asset,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<ProfileAssets> {
    let damage_path = canonical_art_path(asset.damage_file.as_deref()?, Some("dmg"));
    let model_path = canonical_art_path(asset.file.as_deref()?, None);
    let key = damage_path.to_ascii_lowercase();
    if !cache.damage_templates.contains_key(&key) {
        cache
            .damage_templates
            .insert(key.clone(), source.read_xmb(&damage_path));
    }
    let document = cache.damage_templates.get(&key).cloned().flatten()?;
    Some(ProfileAssets {
        model_path,
        document,
    })
}

fn load_geometry(
    path: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<UgxGeom> {
    let key = path.to_ascii_lowercase();
    if !cache.geometry.contains_key(&key) {
        let geometry = source
            .resolve_with_fallback(path, &[".ugx"])
            .and_then(|bytes| Reader::read(&bytes).ok());
        cache.geometry.insert(key.clone(), geometry);
    }
    cache.geometry.get(&key).cloned().flatten()
}

fn impact_point_nodes(document: &Document) -> Vec<&Node> {
    document
        .root()
        .and_then(|root| child(root, "impactpointbased"))
        .map(|node| {
            node.children
                .iter()
                .filter(|child| child.name.eq_ignore_ascii_case("impactpoint"))
                .collect()
        })
        .unwrap_or_default()
}

fn valid_impact_point(node: &Node, geometry: &UgxGeom) -> bool {
    let Some(bone) = attribute(node, "bone") else {
        return false;
    };
    bone.eq_ignore_ascii_case("default")
        || geometry
            .granny_bones
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(&bone))
        || geometry
            .bones
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(&bone))
}

fn parse_final_throw_part(
    impact: &Node,
    geometry: &UgxGeom,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<ThrownDamagePart> {
    let event = impact
        .children
        .iter()
        .rfind(|event| event.name.eq_ignore_ascii_case("event") && !event.children.is_empty())?;
    event
        .children
        .iter()
        .filter(|action| {
            action.name.eq_ignore_ascii_case("action")
                && attribute(action, "type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("throwpart"))
        })
        .find_map(|action| parse_throw_part(action, geometry, source, cache))
}

fn parse_throw_part(
    action: &Node,
    geometry: &UgxGeom,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<ThrownDamagePart> {
    let mesh_names = action
        .text_string()
        .split(',')
        .filter_map(|name| resolve_mesh_name(geometry, name.trim()))
        .collect::<Vec<_>>();
    if mesh_names.is_empty() {
        return None;
    }
    let blueprint = attribute(action, "blueprint").unwrap_or_else(|| "part".to_owned());
    let material = load_material(&blueprint, source, cache)?;
    let force_multiplier = attribute(action, "forcemultiplier")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0);
    let (collider, throw_center_offset) = action_geometry(geometry, &mesh_names)?;
    let mut part = ThrownDamagePart::new(mesh_names, collider, material, force_multiplier)?;
    part.throw_center_offset = throw_center_offset;
    Some(part)
}

fn load_material(
    reference: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AssetCache,
) -> Option<PhysicsMaterial> {
    let key = reference.trim().to_ascii_lowercase();
    if !cache.materials.contains_key(&key) {
        let material = vehicle_physics::load_blueprint_material(source, reference).ok();
        cache.materials.insert(key.clone(), material);
    }
    cache.materials.get(&key).copied().flatten()
}

fn resolve_mesh_name(geometry: &UgxGeom, authored: &str) -> Option<String> {
    geometry
        .granny_meshes
        .iter()
        .find(|mesh| mesh.name.eq_ignore_ascii_case(authored))
        .map(|mesh| mesh.name.clone())
}

fn action_geometry(geometry: &UgxGeom, mesh_names: &[String]) -> Option<(BoxCollider, Vec3)> {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut throw_center = Vec3::ZERO;
    for name in mesh_names {
        let mesh = geometry
            .granny_meshes
            .iter()
            .find(|mesh| mesh.name.eq_ignore_ascii_case(name))?;
        let (mesh_minimum, mesh_maximum) = mesh_bounds(geometry, mesh)?;
        let center = (mesh_minimum + mesh_maximum) * 0.5;
        throw_center += center;
        let half_extents = (mesh_maximum - mesh_minimum) * 0.5 * PART_BOUNDING_BOX_FACTOR;
        minimum = minimum.min(center - half_extents);
        maximum = maximum.max(center + half_extents);
    }
    let center = (minimum + maximum) * 0.5;
    let half_extents = (maximum - minimum) * 0.5;
    let count = u16::try_from(mesh_names.len()).ok()?;
    throw_center /= f32::from(count);
    (center.is_finite() && throw_center.is_finite() && valid_extents(half_extents))
        .then_some((BoxCollider::new(half_extents, center), throw_center))
}

fn mesh_bounds(geometry: &UgxGeom, mesh: &GrannyMesh) -> Option<(Vec3, Vec3)> {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for binding in &mesh.bone_bindings {
        let bone_world = geometry
            .granny_bones
            .iter()
            .find(|bone| bone.name.eq_ignore_ascii_case(&binding.bone_name))
            .and_then(|bone| bone.inverse_world_matrix.inverse());
        for corner in box_corners(
            Vec3::from_array(binding.obb_min),
            Vec3::from_array(binding.obb_max),
        ) {
            let point = bone_world
                .as_ref()
                .map_or(corner, |matrix| transform_point(matrix, corner));
            minimum = minimum.min(point);
            maximum = maximum.max(point);
        }
    }
    (minimum.is_finite() && maximum.is_finite() && valid_extents(maximum - minimum))
        .then_some((minimum, maximum))
}

fn box_corners(minimum: Vec3, maximum: Vec3) -> [Vec3; 8] {
    [
        Vec3::new(minimum.x, minimum.y, minimum.z),
        Vec3::new(maximum.x, minimum.y, minimum.z),
        Vec3::new(minimum.x, maximum.y, minimum.z),
        Vec3::new(maximum.x, maximum.y, minimum.z),
        Vec3::new(minimum.x, minimum.y, maximum.z),
        Vec3::new(maximum.x, minimum.y, maximum.z),
        Vec3::new(minimum.x, maximum.y, maximum.z),
        Vec3::new(maximum.x, maximum.y, maximum.z),
    ]
}

fn transform_point(matrix: &pipeline::ugx::Matrix4x4, point: Vec3) -> Vec3 {
    let rows = &matrix.rows;
    Vec3::new(
        point.x.mul_add(
            rows[0][0],
            point.y.mul_add(rows[1][0], point.z * rows[2][0]),
        ) + rows[3][0],
        point.x.mul_add(
            rows[0][1],
            point.y.mul_add(rows[1][1], point.z * rows[2][1]),
        ) + rows[3][1],
        point.x.mul_add(
            rows[0][2],
            point.y.mul_add(rows[1][2], point.z * rows[2][2]),
        ) + rows[3][2],
    )
}

fn child<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children
        .iter()
        .find(|child| child.name.eq_ignore_ascii_case(name))
}

fn attribute(node: &Node, name: &str) -> Option<String> {
    node.attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))
        .map(pipeline::xmb::Attribute::value_string)
}

fn canonical_art_path(reference: &str, extension: Option<&str>) -> String {
    let mut path = reference.trim().replace('/', "\\");
    path = path.trim_start_matches('\\').to_owned();
    if path.to_ascii_lowercase().ends_with(".xmb") {
        path.truncate(path.len().saturating_sub(4));
    }
    if !path.to_ascii_lowercase().starts_with("art\\") {
        path = format!("art\\{path}");
    }
    if let Some(extension) = extension
        && !path
            .to_ascii_lowercase()
            .ends_with(&format!(".{extension}"))
    {
        path.push('.');
        path.push_str(extension);
    }
    path
}

fn normalized_mesh_names(names: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for name in names {
        let name = name.trim();
        if !name.is_empty()
            && !normalized
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(name))
        {
            normalized.push(name.to_owned());
        }
    }
    normalized
}

fn valid_extents(extents: Vec3) -> bool {
    extents.is_finite() && extents.x > 0.0 && extents.y > 0.0 && extents.z > 0.0
}

fn valid_force_multiplier(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::ugx::{AABB, GeometryFlags, GrannyBone, GrannyBoneBinding, Matrix4x4, Sphere};

    #[test]
    fn empty_infantry_template_preserves_zero_impact_points() {
        let document =
            Document::from_bytes(br"<damagetemplate><impactpointbased/></damagetemplate>").unwrap();
        assert!(impact_point_nodes(&document).is_empty());
    }

    #[test]
    fn final_nonempty_event_selects_first_resolvable_throw_part() {
        let document = Document::from_bytes(
            br#"<damagetemplate><impactpointbased><impactpoint bone="root">
                <event><action type="throwpart">missing</action></event>
                <event><action type="playsound">sound</action>
                <action type="throwpart">panel</action></event>
            </impactpoint></impactpointbased></damagetemplate>"#,
        )
        .unwrap();
        let geometry = geometry();
        let impact = impact_point_nodes(&document)[0];
        let event = impact
            .children
            .iter()
            .rfind(|event| !event.children.is_empty())
            .unwrap();
        let mesh = event
            .children
            .iter()
            .find(|action| attribute(action, "type").as_deref() == Some("throwpart"))
            .and_then(|action| resolve_mesh_name(&geometry, &action.text_string()));
        assert_eq!(mesh.as_deref(), Some("panel"));
    }

    #[test]
    fn granny_bone_obb_becomes_scaled_model_space_collider() {
        let geometry = geometry();
        let (collider, throw_center) = action_geometry(&geometry, &["panel".to_owned()]).unwrap();
        assert!(
            collider
                .center_offset
                .abs_diff_eq(Vec3::new(3.0, 2.0, 3.0), 0.000_1)
        );
        assert!(
            collider
                .half_extents
                .abs_diff_eq(Vec3::splat(0.75), 0.000_1)
        );
        assert!(throw_center.abs_diff_eq(collider.center_offset, 0.000_1));
    }

    #[test]
    fn catalog_lookup_is_case_insensitive() {
        let mut catalog = GameplayCatalog::default();
        catalog.insert_test_damage_parts("Vehicle", DamagePartProfile::default());
        assert_eq!(
            catalog
                .damage_parts("vehicle")
                .unwrap()
                .impact_point_count(),
            0
        );
    }

    fn geometry() -> UgxGeom {
        let bone = GrannyBone {
            name: "root".to_owned(),
            inverse_world_matrix: Matrix4x4 {
                rows: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [-2.0, 0.0, 0.0, 1.0],
                ],
            },
            ..GrannyBone::default()
        };
        UgxGeom {
            bounding_sphere: Sphere::default(),
            bounds: AABB::default(),
            materials: Vec::new(),
            bones: Vec::new(),
            granny_bones: vec![bone],
            granny_meshes: vec![GrannyMesh {
                name: "panel".to_owned(),
                bone_bindings: vec![GrannyBoneBinding {
                    bone_name: "root".to_owned(),
                    obb_min: [0.0, 1.0, 2.0],
                    obb_max: [2.0, 3.0, 4.0],
                    triangle_indices: Vec::new(),
                }],
            }],
            skeleton_lod_type: 0,
            bone_bounds: Vec::new(),
            sections: Vec::new(),
            vertex_buffer: Vec::new(),
            index_buffer: Vec::new(),
            accessories: Vec::new(),
            valid_accessories: Vec::new(),
            rigid_only: false,
            rigid_bone_index: -1,
            max_instances: 1,
            instance_index_multiplier: 0,
            large_geom_bone_index: i16::MAX,
            flags: GeometryFlags::default(),
            aabb_tree: None,
        }
    }
}
