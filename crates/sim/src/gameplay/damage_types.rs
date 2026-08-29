//! Directional armor and shield profiles recovered from layered object data.

use crate::entities::{ShieldCoverage, SquadMode};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::xmb::{Document, Node};
use std::collections::BTreeMap;

const SECTOR_COUNT: usize = 6;
const DIAGONAL_THRESHOLD: f32 = core::f32::consts::FRAC_1_SQRT_2;

#[derive(Debug, Clone, Default)]
pub(super) struct DamageTypeProfiles {
    objects: BTreeMap<String, ObjectDamageProfile>,
}

#[derive(Debug, Clone)]
struct ObjectDamageProfile {
    normal: [Option<String>; SECTOR_COUNT],
    secondary: [Option<String>; SECTOR_COUNT],
    secondary_mode: Option<String>,
    base_damage_type: Option<String>,
    shield_coverage: ShieldCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum DamageSector {
    Front = 0,
    FrontRight = 1,
    BackRight = 2,
    Back = 3,
    BackLeft = 4,
    FrontLeft = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthoredDirection {
    Full,
    FrontHalf,
    BackHalf,
    Right,
    Left,
    Front,
    Back,
}

impl Default for ObjectDamageProfile {
    fn default() -> Self {
        Self {
            normal: std::array::from_fn(|_| None),
            secondary: std::array::from_fn(|_| None),
            secondary_mode: None,
            base_damage_type: None,
            shield_coverage: ShieldCoverage::None,
        }
    }
}

impl DamageTypeProfiles {
    pub(super) fn from_database(database: &Database) -> Self {
        let mut profiles = Self::default();
        for object in &database.objects {
            let Some(damage_type) = object.damage_type.as_deref().map(str::trim) else {
                continue;
            };
            let mut profile = ObjectDamageProfile::default();
            if is_shielded_type(database, damage_type) {
                profile.shield_coverage = ShieldCoverage::Full;
            } else if !damage_type.is_empty() {
                profile.add_armor(damage_type, AuthoredDirection::Full, false);
                profile.base_damage_type = Some(damage_type.to_owned());
            }
            profiles
                .objects
                .insert(object.name.to_ascii_lowercase(), profile);
        }
        profiles
    }

    pub(super) fn from_document(database: &Database, document: &Document) -> Self {
        let mut profiles = Self::from_database(database);
        let Some(root) = document.root() else {
            return profiles;
        };
        for object in root
            .children
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case("Object"))
        {
            let Some(name) = attribute(object, "name") else {
                continue;
            };
            let Some(profile) = ObjectDamageProfile::from_node(database, object) else {
                continue;
            };
            profiles.objects.insert(name.to_ascii_lowercase(), profile);
        }
        profiles
    }

    pub(super) fn base_damage_type(&self, proto_object: &str) -> Option<&str> {
        self.objects
            .get(&proto_object.to_ascii_lowercase())?
            .base_damage_type
            .as_deref()
    }

    pub(super) fn base_damage_types(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.objects.iter().filter_map(|(name, profile)| {
            Some((name.as_str(), profile.base_damage_type.as_deref()?))
        })
    }

    pub(super) fn shield_coverage(&self, proto_object: &str) -> ShieldCoverage {
        self.objects
            .get(&proto_object.to_ascii_lowercase())
            .map_or(ShieldCoverage::None, |profile| profile.shield_coverage)
    }

    pub(super) fn shield_coverages(&self) -> impl Iterator<Item = (&str, ShieldCoverage)> + '_ {
        self.objects
            .iter()
            .map(|(name, profile)| (name.as_str(), profile.shield_coverage))
    }

    pub(super) fn damage_type(
        &self,
        proto_object: &str,
        direction: Vec3,
        forward: Vec3,
        mode: SquadMode,
    ) -> Option<&str> {
        self.objects
            .get(&proto_object.to_ascii_lowercase())?
            .damage_type(direction, forward, mode)
    }
}

impl ObjectDamageProfile {
    fn from_node(database: &Database, object: &Node) -> Option<Self> {
        let mut profile = Self::default();
        let mut found = false;
        for node in object
            .children
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case("DamageType"))
        {
            let damage_type = node.text_string();
            let damage_type = damage_type.trim();
            if damage_type.is_empty() || !is_known_type(database, damage_type) {
                continue;
            }
            found = true;
            let direction = attribute(node, "direction")
                .as_deref()
                .and_then(AuthoredDirection::parse);
            if is_shielded_type(database, damage_type) {
                profile.add_shield(direction);
                continue;
            }
            let secondary = attribute(node, "mode")
                .filter(|mode| !mode.is_empty() && !mode.eq_ignore_ascii_case("Normal"));
            if let Some(mode) = secondary.as_deref()
                && profile.secondary_mode.is_none()
            {
                profile.secondary_mode = Some(mode.to_owned());
            }
            if let Some(direction) = direction {
                profile.add_armor(damage_type, direction, secondary.is_some());
            }
            if secondary.is_none()
                && (direction == Some(AuthoredDirection::Full)
                    || profile.base_damage_type.is_none())
                && is_base_type(database, damage_type)
            {
                profile.base_damage_type = Some(damage_type.to_owned());
            }
        }
        found.then_some(profile)
    }

    fn add_shield(&mut self, direction: Option<AuthoredDirection>) {
        if self.shield_coverage != ShieldCoverage::None {
            return;
        }
        self.shield_coverage = match direction {
            Some(AuthoredDirection::Full) => ShieldCoverage::Full,
            Some(AuthoredDirection::FrontHalf) => ShieldCoverage::FrontHalf,
            _ => ShieldCoverage::None,
        };
    }

    fn add_armor(&mut self, damage_type: &str, direction: AuthoredDirection, secondary: bool) {
        let slots = if secondary {
            &mut self.secondary
        } else {
            &mut self.normal
        };
        for sector in direction.sectors() {
            let slot = &mut slots[*sector as usize];
            if slot.is_none() {
                *slot = Some(damage_type.to_owned());
            }
        }
    }

    fn damage_type(&self, direction: Vec3, forward: Vec3, mode: SquadMode) -> Option<&str> {
        let sector = DamageSector::classify(direction, forward) as usize;
        let uses_secondary = self
            .secondary_mode
            .as_deref()
            .is_some_and(|authored| authored.eq_ignore_ascii_case(mode.as_str()));
        if uses_secondary && let Some(damage_type) = self.secondary[sector].as_deref() {
            return Some(damage_type);
        }
        self.normal[sector].as_deref()
    }
}

impl DamageSector {
    fn classify(direction: Vec3, forward: Vec3) -> Self {
        let direction = direction.normalize_or_zero();
        let forward = forward.normalize_or_zero();
        let right = Vec3::Y.cross(forward).normalize_or_zero();
        let forward_dot = direction.dot(forward);
        if forward_dot <= -DIAGONAL_THRESHOLD {
            Self::Front
        } else if forward_dot <= 0.0 {
            if direction.dot(right) <= 0.0 {
                Self::FrontRight
            } else {
                Self::FrontLeft
            }
        } else if forward_dot >= DIAGONAL_THRESHOLD {
            Self::Back
        } else if direction.dot(right) <= 0.0 {
            Self::BackRight
        } else {
            Self::BackLeft
        }
    }
}

impl AuthoredDirection {
    fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("Full") {
            Some(Self::Full)
        } else if value.eq_ignore_ascii_case("FrontHalf") {
            Some(Self::FrontHalf)
        } else if value.eq_ignore_ascii_case("BackHalf") {
            Some(Self::BackHalf)
        } else if value.eq_ignore_ascii_case("Right") {
            Some(Self::Right)
        } else if value.eq_ignore_ascii_case("Left") {
            Some(Self::Left)
        } else if value.eq_ignore_ascii_case("Front") {
            Some(Self::Front)
        } else if value.eq_ignore_ascii_case("Back") {
            Some(Self::Back)
        } else {
            None
        }
    }

    fn sectors(self) -> &'static [DamageSector] {
        use DamageSector::{Back, BackLeft, BackRight, Front, FrontLeft, FrontRight};
        match self {
            Self::Full => &[Front, FrontRight, BackRight, Back, BackLeft, FrontLeft],
            Self::FrontHalf => &[FrontLeft, Front, FrontRight],
            Self::BackHalf => &[BackLeft, Back, BackRight],
            Self::Right => &[FrontRight, BackRight],
            Self::Left => &[FrontLeft, BackLeft],
            Self::Front => &[Front],
            Self::Back => &[Back],
        }
    }
}

fn attribute(node: &Node, name: &str) -> Option<String> {
    node.attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))
        .map(pipeline::xmb::Attribute::value_string)
}

fn is_known_type(database: &Database, name: &str) -> bool {
    database.damage_types.is_empty()
        || database
            .damage_types
            .iter()
            .any(|damage_type| damage_type.name.eq_ignore_ascii_case(name))
}

fn is_base_type(database: &Database, name: &str) -> bool {
    database
        .damage_types
        .iter()
        .find(|damage_type| damage_type.name.eq_ignore_ascii_case(name))
        .map_or(!is_shielded_type(database, name), |damage_type| {
            damage_type.base_type.unwrap_or(false)
        })
}

fn is_shielded_type(database: &Database, name: &str) -> bool {
    name.eq_ignore_ascii_case("Shielded")
        || database.damage_types.iter().any(|damage_type| {
            damage_type.name.eq_ignore_ascii_case(name) && damage_type.shielded.unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::DamageType;

    fn database() -> Database {
        let mut database = Database::new();
        for name in [
            "Light",
            "LightInCover",
            "FrontArmor",
            "BackArmor",
            "RightArmor",
            "LeftArmor",
        ] {
            database.damage_types.push(DamageType {
                name: name.to_owned(),
                base_type: Some(true),
                ..DamageType::default()
            });
        }
        database.damage_types.push(DamageType {
            name: "Shielded".to_owned(),
            shielded: Some(true),
            ..DamageType::default()
        });
        database
    }

    fn profiles(xml: &str) -> DamageTypeProfiles {
        let document = Document::from_xml(xml).expect("valid object XML");
        DamageTypeProfiles::from_document(&database(), &document)
    }

    #[test]
    fn raw_profiles_preserve_front_shields_and_secondary_modes() {
        let profiles = profiles(
            r#"<Objects><Object name="jackal">
                <DamageType direction="FrontHalf">Shielded</DamageType>
                <DamageType direction="Full" mode="Normal">Light</DamageType>
                <DamageType direction="Full" mode="Cover">LightInCover</DamageType>
            </Object></Objects>"#,
        );

        assert_eq!(
            profiles.shield_coverage("JACKAL"),
            ShieldCoverage::FrontHalf
        );
        assert_eq!(profiles.base_damage_type("jackal"), Some("Light"));
        assert_eq!(
            profiles.damage_type("jackal", Vec3::NEG_Z, Vec3::Z, SquadMode::Normal),
            Some("Light")
        );
        assert_eq!(
            profiles.damage_type("jackal", Vec3::NEG_Z, Vec3::Z, SquadMode::Cover),
            Some("LightInCover")
        );
    }

    #[test]
    fn impact_vectors_select_retail_six_way_armor_sectors() {
        let profiles = profiles(
            r#"<Objects><Object name="target">
                <DamageType direction="Front">FrontArmor</DamageType>
                <DamageType direction="Back">BackArmor</DamageType>
                <DamageType direction="Right">RightArmor</DamageType>
                <DamageType direction="Left">LeftArmor</DamageType>
            </Object></Objects>"#,
        );
        let armor =
            |direction| profiles.damage_type("target", direction, Vec3::Z, SquadMode::Normal);

        assert_eq!(armor(Vec3::NEG_Z), Some("FrontArmor"));
        assert_eq!(armor(Vec3::Z), Some("BackArmor"));
        assert_eq!(armor(Vec3::NEG_X), Some("RightArmor"));
        assert_eq!(armor(Vec3::X), Some("LeftArmor"));
        assert_eq!(armor(Vec3::new(-1.0, 0.0, 0.5)), Some("RightArmor"));
        assert_eq!(armor(Vec3::new(1.0, 0.0, 0.5)), Some("LeftArmor"));
    }
}
