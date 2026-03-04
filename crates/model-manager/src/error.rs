use std::path::PathBuf;

use crate::ModelId;

#[derive(Debug, thiserror::Error)]
pub enum ModelManagerError {
    #[error("Model not found: {0}")]
    NotFound(ModelId),

    #[error("SHA256 verification failed for {path}: expected {expected}, got {actual}")]
    Sha256Mismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("Model file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Registry error: {0}")]
    Registry(String),
}
