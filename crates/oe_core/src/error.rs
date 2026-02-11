//! Error types for OpenEnsemble

use thiserror::Error;

/// Core error type for the engine
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Format error: {0}")]
    Format(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Initialization error: {0}")]
    Init(String),

    #[error("{0}")]
    Other(String),
}

/// Result type alias using our Error
pub type Result<T> = std::result::Result<T, Error>;

