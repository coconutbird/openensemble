//! Error types for `OpenEnsemble`

use thiserror::Error;

/// Core error type for the engine
#[derive(Error, Debug)]
pub enum Error {
    /// An I/O operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Data has an incorrect format.
    #[error("Format error: {0}")]
    Format(String),

    /// A requested resource does not exist.
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// Initialization of a subsystem failed.
    #[error("Initialization error: {0}")]
    Init(String),

    /// Other error with a message.
    #[error("{0}")]
    Other(String),
}

/// Result type alias using our Error
pub type Result<T> = std::result::Result<T, Error>;
