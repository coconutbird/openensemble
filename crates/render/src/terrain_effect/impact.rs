//! Impact-effect prototype names and their terrain-effect routes.

use std::borrow::Cow;

use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xmb::{Document, Node, Reader, Variant};

const IMPACT_EFFECTS_PATH: &str = "data\\impacteffects.xml";
const DEFAULT_LIFESPAN_SECONDS: f32 = 3.0;
const DEFAULT_METER_LIMIT: u32 = 2;
const RETAIL_BOUNDING_RADIUS: f32 = 10.0;

/// One named prototype from `impacteffects.xml`.
#[derive(Clone, Debug, PartialEq)]
pub struct ImpactEffectDefinition {
    /// Case-insensitive name referenced by tactic `ImpactEffect` nodes.
    pub name: String,
    /// TFX asset instantiated when the impact is presented.
    pub terrain_effect_path: String,
    /// Lifetime forwarded to temporary visuals and the TFX meter.
    pub lifespan_seconds: f32,
    /// Maximum simultaneous metered instances in one retail meter window.
    pub meter_limit: u32,
    /// Retail's fixed spatial-meter radius.
    pub bounding_radius: f32,
}

/// Ordered impact-effect prototype table loaded before tactics in retail.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImpactEffectCatalog {
    definitions: Vec<ImpactEffectDefinition>,
}

impl ImpactEffectCatalog {
    /// Load `data\\impacteffects.xml` from the active asset stack.
    ///
    /// # Errors
    ///
    /// Returns an error when the document is absent or malformed, or a terrain
    /// effect entry lacks its required name/path.
    pub fn load(source: &mut AssetSource<StdFileProvider>) -> Result<Self, ImpactEffectError> {
        let bytes = source
            .resolve_with_fallback(IMPACT_EFFECTS_PATH, &[".xmb"])
            .ok_or(ImpactEffectError::NotFound)?;
        let document = Reader::read(&bytes).map_err(|error| ImpactEffectError::Parse {
            reason: error.to_string(),
        })?;
        Self::from_document(&document)
    }

    /// Adapt an already decoded impact-effect document.
    ///
    /// Retail ignores unknown root children, so this does the same while
    /// retaining every `TerrainEffect` child in authored order.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing root or an incomplete entry.
    pub fn from_document(document: &Document) -> Result<Self, ImpactEffectError> {
        let root = document.root().ok_or(ImpactEffectError::MissingRoot)?;
        let definitions = root
            .children
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case("TerrainEffect"))
            .map(ImpactEffectDefinition::from_node)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { definitions })
    }

    /// Resolve a tactic's impact-effect name case-insensitively.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ImpactEffectDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name.eq_ignore_ascii_case(name))
    }

    /// Return definitions in retail database order.
    #[must_use]
    pub fn definitions(&self) -> &[ImpactEffectDefinition] {
        &self.definitions
    }

    /// Return the number of named prototypes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Return whether the catalog has no prototypes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

impl ImpactEffectDefinition {
    fn from_node(node: &Node) -> Result<Self, ImpactEffectError> {
        let name = attribute(node, "name")
            .map(Cow::into_owned)
            .filter(|name| !name.trim().is_empty())
            .ok_or(ImpactEffectError::MissingName)?;
        let terrain_effect_path = node_text(node).into_owned().trim().to_owned();
        if terrain_effect_path.is_empty() {
            return Err(ImpactEffectError::MissingTerrainEffectPath { name });
        }
        Ok(Self {
            name,
            terrain_effect_path,
            lifespan_seconds: parse_attribute(node, "lifespan").unwrap_or(DEFAULT_LIFESPAN_SECONDS),
            meter_limit: parse_attribute(node, "limit").unwrap_or(DEFAULT_METER_LIMIT),
            bounding_radius: RETAIL_BOUNDING_RADIUS,
        })
    }
}

/// Failure to decode the retail impact-effect prototype table.
#[derive(Debug, thiserror::Error)]
pub enum ImpactEffectError {
    /// The active asset stack does not contain the global prototype table.
    #[error("impact-effect catalog not found: {IMPACT_EFFECTS_PATH}")]
    NotFound,
    /// The generic XML/XMB reader rejected the source bytes.
    #[error("failed to parse impact-effect catalog: {reason}")]
    Parse {
        /// Reader diagnostic.
        reason: String,
    },
    /// The decoded document contained no root node.
    #[error("impact-effect catalog has no root node")]
    MissingRoot,
    /// A `TerrainEffect` entry omitted its database key.
    #[error("impact-effect definition is missing its name")]
    MissingName,
    /// A named prototype omitted its routed TFX path.
    #[error("impact-effect definition '{name}' has no terrain-effect path")]
    MissingTerrainEffectPath {
        /// Prototype name retained for diagnostics.
        name: String,
    },
}

fn node_text(node: &Node) -> Cow<'_, str> {
    match &node.text {
        Variant::String(value) => Cow::Borrowed(value.as_str()),
        Variant::Null => Cow::Borrowed(""),
        _ => Cow::Owned(node.text_string()),
    }
}

fn attribute<'node>(node: &'node Node, name: &str) -> Option<Cow<'node, str>> {
    let attribute = node
        .attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))?;
    match &attribute.value {
        Variant::String(value) => Some(Cow::Borrowed(value.as_str())),
        Variant::Null => None,
        _ => Some(Cow::Owned(attribute.value_string())),
    }
}

fn parse_attribute<T: std::str::FromStr>(node: &Node, name: &str) -> Option<T> {
    attribute(node, name)?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use pipeline::xmb::Document;

    use super::{ImpactEffectCatalog, RETAIL_BOUNDING_RADIUS};

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn parses_defaults_and_case_insensitive_names() {
        let document = Document::from_xml(
            r#"<ImpactEffects>
                <Ignored />
                <TerrainEffect name="Impact">effects\impact</TerrainEffect>
                <TerrainEffect name="Heavy" lifespan="1.25" limit="7">
                    effects\heavy
                </TerrainEffect>
            </ImpactEffects>"#,
        )
        .unwrap();
        let catalog = ImpactEffectCatalog::from_document(&document).unwrap();

        assert_eq!(catalog.len(), 2);
        let impact = catalog.get("IMPACT").unwrap();
        assert_eq!(impact.terrain_effect_path, "effects\\impact");
        assert_near(impact.lifespan_seconds, 3.0);
        assert_eq!(impact.meter_limit, 2);
        assert_near(impact.bounding_radius, RETAIL_BOUNDING_RADIUS);
        let heavy = catalog.get("heavy").unwrap();
        assert_eq!(heavy.terrain_effect_path, "effects\\heavy");
        assert_near(heavy.lifespan_seconds, 1.25);
        assert_eq!(heavy.meter_limit, 7);
    }
}
