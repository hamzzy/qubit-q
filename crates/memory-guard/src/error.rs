#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Insufficient memory: need {required} bytes, only {available} bytes safe to use")]
    InsufficientMemory {
        required: u64,
        available: u64,
        suggestion: Option<String>,
    },

    #[error("System memory detection failed: {0}")]
    DetectionFailed(String),

    #[error("Eviction not supported in this phase")]
    EvictionNotSupported,
}
