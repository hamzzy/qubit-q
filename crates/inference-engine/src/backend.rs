use std::path::Path;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use model_manager::GenerationParams;

use crate::error::InferenceError;

/// A single generated token with metadata.
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub id: u32,
    pub logprob: Option<f32>,
}

/// Statistics returned after completion finishes.
#[derive(Debug, Clone)]
pub struct CompletionStats {
    pub tokens_generated: usize,
    pub tokens_per_second: f32,
    pub prompt_tokens: usize,
    pub total_duration_ms: u64,
}

/// Configuration for loading a model into the backend.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub path: std::path::PathBuf,
    pub context_size: usize,
    pub gpu_layers: Option<u32>,
}

/// Trait for inference backends (llama.cpp, mock, etc.).
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// Load a model from disk.
    async fn load_model(&mut self, path: &Path, config: &ModelConfig)
        -> Result<(), InferenceError>;

    /// Unload model and free all memory.
    async fn unload_model(&mut self) -> Result<(), InferenceError>;

    /// Stream completion tokens. Sends each token to `tx` as generated.
    /// Respects `cancel` token.
    async fn stream_completion(
        &self,
        prompt: &str,
        params: &GenerationParams,
        tx: mpsc::Sender<Token>,
        cancel: CancellationToken,
    ) -> Result<CompletionStats, InferenceError>;

    /// Current memory footprint of loaded model in bytes.
    fn memory_usage_bytes(&self) -> u64;

    /// Is a model currently loaded?
    fn is_loaded(&self) -> bool;
}
