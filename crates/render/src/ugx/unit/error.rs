use super::MAX_ATTACHMENT_DEPTH;
use crate::ugx::LoadError;

/// Errors produced while resolving a visual's recursive model graph.
#[derive(Debug, thiserror::Error)]
pub enum UnitLoadError {
    /// The visual does not name a root model.
    #[error("visual has no default model")]
    MissingDefaultModel,
    /// A named model reference was absent from the visual.
    #[error("visual model reference not found: {0}")]
    ModelReferenceNotFound(String),
    /// A visual model had no direct UGX model asset.
    #[error("visual model '{0}' has no model asset")]
    ModelAssetMissing(String),
    /// A component UGX failed to resolve or decode.
    #[error("failed to load visual model '{component}' from '{path}': {source}")]
    Model {
        /// Named component in the visual graph.
        component: String,
        /// Resolved game asset path.
        path: String,
        /// Underlying UGX error.
        #[source]
        source: LoadError,
    },
    /// The model-reference graph contains a cycle or is unreasonably deep.
    #[error("visual attachment graph exceeds {MAX_ATTACHMENT_DEPTH} levels")]
    AttachmentDepthExceeded,
    /// The decoded visual contains an attachment outside the shipped schema.
    #[error("unsupported visual attachment type '{0}'")]
    UnsupportedAttachmentType(String),
}
