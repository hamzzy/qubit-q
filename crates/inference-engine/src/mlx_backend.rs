use std::path::Path;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use model_manager::GenerationParams;

use crate::{CompletionStats, InferenceBackend, InferenceError, ModelConfig, Token};

/// MLX backend scaffold for Apple platforms.
///
/// This is intentionally minimal: it compiles and wires feature-gates now,
/// and returns explicit NotSupported errors until full MLX graph/tokenizer
/// integration is completed.
#[derive(Debug, Default)]
pub struct MlxBackend {
    loaded: bool,
}

impl MlxBackend {
    pub fn new() -> Self {
        Self { loaded: false }
    }
}

#[async_trait]
impl InferenceBackend for MlxBackend {
    async fn load_model(
        &mut self,
        _path: &Path,
        _config: &ModelConfig,
    ) -> Result<(), InferenceError> {
        Err(InferenceError::NotSupported(
            "MLX backend scaffold is enabled, but model loading is not implemented yet".into(),
        ))
    }

    async fn unload_model(&mut self) -> Result<(), InferenceError> {
        self.loaded = false;
        Ok(())
    }

    async fn stream_completion(
        &self,
        _prompt: &str,
        _params: &GenerationParams,
        _tx: mpsc::Sender<Token>,
        _cancel: CancellationToken,
    ) -> Result<CompletionStats, InferenceError> {
        Err(InferenceError::NotSupported(
            "MLX backend scaffold is enabled, but token streaming is not implemented yet".into(),
        ))
    }

    fn memory_usage_bytes(&self) -> u64 {
        0
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }
}
