use std::path::Path;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use model_manager::GenerationParams;

use crate::backend::{CompletionStats, InferenceBackend, ModelConfig, Token};
use crate::error::InferenceError;

/// Mock backend for testing without a real GGUF model.
pub struct MockBackend {
    loaded: bool,
    response_tokens: Vec<String>,
    delay_per_token_ms: u64,
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            loaded: false,
            response_tokens: "Hello! I am a mock AI assistant running locally on your device. How can I help you today?"
                .split_whitespace()
                .map(|s| format!("{s} "))
                .collect(),
            delay_per_token_ms: 30,
        }
    }

    pub fn with_response(mut self, tokens: Vec<String>) -> Self {
        self.response_tokens = tokens;
        self
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_per_token_ms = delay_ms;
        self
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InferenceBackend for MockBackend {
    async fn load_model(
        &mut self,
        path: &Path,
        _config: &ModelConfig,
    ) -> Result<(), InferenceError> {
        info!(path = %path.display(), "Mock: loading model");
        self.loaded = true;
        Ok(())
    }

    async fn unload_model(&mut self) -> Result<(), InferenceError> {
        info!("Mock: unloading model");
        self.loaded = false;
        Ok(())
    }

    async fn stream_completion(
        &self,
        prompt: &str,
        params: &GenerationParams,
        tx: mpsc::Sender<Token>,
        cancel: CancellationToken,
    ) -> Result<CompletionStats, InferenceError> {
        if !self.loaded {
            return Err(InferenceError::NoModelLoaded);
        }

        info!(prompt_len = prompt.len(), "Mock: starting completion");
        let start = Instant::now();
        let max_tokens = params.max_tokens.min(self.response_tokens.len());
        let mut generated = 0;

        for (i, token_text) in self.response_tokens.iter().take(max_tokens).enumerate() {
            if cancel.is_cancelled() {
                return Err(InferenceError::Cancelled);
            }

            let token = Token {
                text: token_text.clone(),
                id: i as u32,
                logprob: None,
            };

            if tx.send(token).await.is_err() {
                break; // Receiver dropped
            }

            generated += 1;

            if self.delay_per_token_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_per_token_ms))
                    .await;
            }
        }

        let elapsed = start.elapsed();
        Ok(CompletionStats {
            tokens_generated: generated,
            tokens_per_second: if elapsed.as_secs_f32() > 0.0 {
                generated as f32 / elapsed.as_secs_f32()
            } else {
                0.0
            },
            prompt_tokens: prompt.split_whitespace().count(),
            total_duration_ms: elapsed.as_millis() as u64,
        })
    }

    fn memory_usage_bytes(&self) -> u64 {
        0
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_load_and_infer() {
        let mut backend = MockBackend::new().with_delay(0);
        let config = ModelConfig {
            path: "/tmp/test.gguf".into(),
            context_size: 2048,
            gpu_layers: None,
        };

        backend
            .load_model(Path::new("/tmp/test.gguf"), &config)
            .await
            .unwrap();
        assert!(backend.is_loaded());

        let (tx, mut rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let params = GenerationParams::default();

        let stats = backend
            .stream_completion("Hello", &params, tx, cancel)
            .await
            .unwrap();

        assert!(stats.tokens_generated > 0);

        let mut tokens = vec![];
        while let Some(token) = rx.recv().await {
            tokens.push(token.text);
        }
        assert!(!tokens.is_empty());
    }

    #[tokio::test]
    async fn test_mock_cancellation() {
        let mut backend = MockBackend::new().with_delay(100);
        let config = ModelConfig {
            path: "/tmp/test.gguf".into(),
            context_size: 2048,
            gpu_layers: None,
        };

        backend
            .load_model(Path::new("/tmp/test.gguf"), &config)
            .await
            .unwrap();

        let (tx, _rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        cancel.cancel(); // Cancel immediately

        let result = backend
            .stream_completion("Hello", &GenerationParams::default(), tx, cancel)
            .await;

        assert!(matches!(result, Err(InferenceError::Cancelled)));
    }
}
