#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("Failed to load model: {0}")]
    ModelLoadFailed(String),

    #[error("No model loaded")]
    NoModelLoaded,

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Inference cancelled")]
    Cancelled,

    #[error("Inference timed out after {0} seconds")]
    Timeout(u64),

    #[error("Feature not supported: {0}")]
    NotSupported(String),

    #[error("Token channel closed")]
    ChannelClosed,
}
