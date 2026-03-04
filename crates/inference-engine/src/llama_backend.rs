use std::num::NonZero;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use model_manager::GenerationParams;

use crate::backend::{CompletionStats, InferenceBackend, ModelConfig, Token};
use crate::error::InferenceError;

/// Wrapper to send non-Send types across thread boundaries.
/// SAFETY: All access to the inner value is serialized by an external Mutex.
/// The llama.cpp model is safe to use from a single thread at a time after loading.
struct SendWrapper<T>(T);
unsafe impl<T> Send for SendWrapper<T> {}
unsafe impl<T> Sync for SendWrapper<T> {}

/// llama.cpp inference backend using the llama-cpp-2 crate.
pub struct LlamaBackendWrapper {
    /// The llama.cpp backend handle, wrapped for Send.
    backend: Arc<SendWrapper<LlamaBackend>>,
    /// The loaded model, wrapped for Send.
    model: Option<Arc<SendWrapper<LlamaModel>>>,
    context_size: usize,
    model_size_bytes: u64,
}

impl LlamaBackendWrapper {
    pub fn new() -> Result<Self, InferenceError> {
        let backend = LlamaBackend::init().map_err(|e| {
            InferenceError::ModelLoadFailed(format!("Failed to init llama backend: {e}"))
        })?;

        Ok(Self {
            backend: Arc::new(SendWrapper(backend)),
            model: None,
            context_size: 2048,
            model_size_bytes: 0,
        })
    }
}

#[async_trait]
impl InferenceBackend for LlamaBackendWrapper {
    async fn load_model(
        &mut self,
        path: &Path,
        config: &ModelConfig,
    ) -> Result<(), InferenceError> {
        let path_buf = path.to_path_buf();
        self.context_size = config.context_size;

        info!(path = %path_buf.display(), ctx = config.context_size, "Loading model with llama.cpp");

        let backend = self.backend.clone();

        // Load model on a blocking thread.
        // LlamaModelParams is created inside the closure to avoid Send issues.
        let model = tokio::task::spawn_blocking(move || {
            let model_params = LlamaModelParams::default();
            let model = LlamaModel::load_from_file(&backend.0, &path_buf, &model_params)
                .map_err(|e| InferenceError::ModelLoadFailed(format!("{e}")))?;
            Ok::<_, InferenceError>(SendWrapper(model))
        })
        .await
        .map_err(|e| InferenceError::ModelLoadFailed(format!("Task join error: {e}")))??;

        self.model_size_bytes = std::fs::metadata(&config.path)
            .map(|m| m.len())
            .unwrap_or(0);

        self.model = Some(Arc::new(model));
        info!("Model loaded successfully");
        Ok(())
    }

    async fn unload_model(&mut self) -> Result<(), InferenceError> {
        self.model = None;
        self.model_size_bytes = 0;
        info!("Model unloaded");
        Ok(())
    }

    async fn stream_completion(
        &self,
        prompt: &str,
        params: &GenerationParams,
        tx: mpsc::Sender<Token>,
        cancel: CancellationToken,
    ) -> Result<CompletionStats, InferenceError> {
        let model = self
            .model
            .as_ref()
            .ok_or(InferenceError::NoModelLoaded)?
            .clone();

        let backend = self.backend.clone();
        let prompt = prompt.to_string();
        let params = params.clone();
        let ctx_size = self.context_size;

        tokio::task::spawn_blocking(move || {
            run_inference_loop(&backend.0, &model.0, &prompt, &params, ctx_size, tx, cancel)
        })
        .await
        .map_err(|e| InferenceError::InferenceFailed(format!("Task join error: {e}")))?
    }

    fn memory_usage_bytes(&self) -> u64 {
        self.model_size_bytes
    }

    fn is_loaded(&self) -> bool {
        self.model.is_some()
    }
}

/// The core inference loop, runs on a blocking thread.
fn run_inference_loop(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &str,
    params: &GenerationParams,
    ctx_size: usize,
    tx: mpsc::Sender<Token>,
    cancel: CancellationToken,
) -> Result<CompletionStats, InferenceError> {
    let ctx_params = LlamaContextParams::default().with_n_ctx(NonZero::new(ctx_size as u32));
    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| InferenceError::InferenceFailed(format!("Context creation failed: {e}")))?;

    let tokens = model
        .str_to_token(prompt, AddBos::Always)
        .map_err(|e| InferenceError::InferenceFailed(format!("Tokenization failed: {e}")))?;

    let prompt_token_count = tokens.len();
    debug!(prompt_tokens = prompt_token_count, "Prompt tokenized");

    if prompt_token_count >= ctx_size {
        return Err(InferenceError::InferenceFailed(format!(
            "Prompt ({prompt_token_count} tokens) exceeds context size ({ctx_size})"
        )));
    }

    let mut batch = LlamaBatch::new(ctx_size, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        batch
            .add(token, i as i32, &[0], is_last)
            .map_err(|e| InferenceError::InferenceFailed(format!("Batch add failed: {e}")))?;
    }

    ctx.decode(&mut batch)
        .map_err(|e| InferenceError::InferenceFailed(format!("Prompt decode failed: {e}")))?;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(params.temperature),
        LlamaSampler::top_k(params.top_k as i32),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::dist(params.seed.unwrap_or(0) as u32),
    ]);

    let start = Instant::now();
    let mut n_cur = tokens.len();
    let mut generated = 0;
    let max_tokens = params.max_tokens.min(ctx_size - prompt_token_count);

    while generated < max_tokens {
        if cancel.is_cancelled() {
            info!(generated, "Inference cancelled");
            return Err(InferenceError::Cancelled);
        }

        let token_id = sampler.sample(&ctx, batch.n_tokens() - 1);

        if model.is_eog_token(token_id) {
            debug!("End of generation token received");
            break;
        }

        let piece_bytes = model
            .token_to_piece_bytes(token_id, 128, true, None)
            .map_err(|e| InferenceError::InferenceFailed(format!("Token decode failed: {e}")))?;

        let piece_str = String::from_utf8_lossy(&piece_bytes).to_string();

        if !piece_str.is_empty() {
            let token = Token {
                text: piece_str,
                id: token_id.0 as u32,
                logprob: None,
            };
            if tx.blocking_send(token).is_err() {
                debug!("Receiver dropped, stopping generation");
                break;
            }
        }

        batch.clear();
        batch
            .add(token_id, n_cur as i32, &[0], true)
            .map_err(|e| InferenceError::InferenceFailed(format!("Batch add failed: {e}")))?;

        ctx.decode(&mut batch)
            .map_err(|e| InferenceError::InferenceFailed(format!("Decode failed: {e}")))?;

        n_cur += 1;
        generated += 1;
    }

    let elapsed = start.elapsed();
    let stats = CompletionStats {
        tokens_generated: generated,
        tokens_per_second: if elapsed.as_secs_f32() > 0.0 {
            generated as f32 / elapsed.as_secs_f32()
        } else {
            0.0
        },
        prompt_tokens: prompt_token_count,
        total_duration_ms: elapsed.as_millis() as u64,
    };

    info!(
        tokens = stats.tokens_generated,
        tps = format!("{:.1}", stats.tokens_per_second),
        duration_ms = stats.total_duration_ms,
        "Inference complete"
    );

    Ok(stats)
}
