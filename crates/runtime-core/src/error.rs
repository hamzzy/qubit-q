#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Model error: {0}")]
    ModelManager(#[from] model_manager::ModelManagerError),

    #[error("Memory error: {0}")]
    Memory(#[from] memory_guard::MemoryError),

    #[error("Inference error: {0}")]
    Inference(#[from] inference_engine::InferenceError),

    #[error("No model loaded")]
    NoModelLoaded,

    #[error("Config error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
