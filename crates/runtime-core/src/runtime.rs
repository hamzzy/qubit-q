use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use inference_engine::{InferenceBackend, ModelConfig, Token};
use memory_guard::{MemoryGuard, WatermarkGuard};
use model_manager::{GenerationParams, InMemoryRegistry, ModelId, ModelMetadata, ModelRegistry};

use crate::config::RuntimeConfig;
use crate::error::RuntimeError;

/// The core runtime orchestrator.
pub struct Runtime {
    config: RuntimeConfig,
    engine: Arc<Mutex<Box<dyn InferenceBackend>>>,
    loaded_model: Arc<Mutex<Option<ModelMetadata>>>,
    registry: InMemoryRegistry,
    memory_guard: WatermarkGuard,
}

impl Runtime {
    /// Create a new Runtime with the given config and inference backend.
    pub async fn new(
        config: RuntimeConfig,
        backend: Box<dyn InferenceBackend>,
    ) -> Result<Self, RuntimeError> {
        config.ensure_dirs()?;

        let registry = InMemoryRegistry::new(&config.models_dir)?;
        let memory_guard =
            WatermarkGuard::new(config.africa_mode, Some(config.memory_safety_margin_pct));

        info!(
            models_dir = %config.models_dir.display(),
            africa_mode = config.africa_mode,
            "Runtime initialized"
        );

        Ok(Self {
            config,
            engine: Arc::new(Mutex::new(backend)),
            loaded_model: Arc::new(Mutex::new(None)),
            registry,
            memory_guard,
        })
    }

    /// Load a model by ID from the registry.
    pub async fn load_model(&self, model_id: &str) -> Result<(), RuntimeError> {
        let id = ModelId::from(model_id);
        let metadata = self
            .registry
            .get(&id)
            .await?
            .ok_or_else(|| model_manager::ModelManagerError::NotFound(id.clone()))?;

        // Verify file exists
        if !metadata.path.exists() {
            return Err(
                model_manager::ModelManagerError::FileNotFound(metadata.path.clone()).into(),
            );
        }

        // Memory check
        self.memory_guard.can_load_model(&metadata)?;

        // SHA256 verification
        info!(model = %id, "Verifying model integrity");
        model_manager::verify_sha256(&metadata.path, &metadata.sha256).await?;

        // Load into engine
        let model_config = ModelConfig {
            path: metadata.path.clone(),
            context_size: self.config.max_context_tokens.min(metadata.context_limit),
            gpu_layers: None,
        };

        let mut engine = self.engine.lock().await;
        engine.load_model(&metadata.path, &model_config).await?;

        // Update state
        *self.loaded_model.lock().await = Some(metadata);
        self.registry.update_last_used(&id).await?;

        info!(model = %id, "Model loaded and ready");
        Ok(())
    }

    /// Load a model directly from a file path (ad-hoc, no registry lookup).
    pub async fn load_model_from_path(
        &self,
        path: &std::path::Path,
        model_id: &str,
    ) -> Result<(), RuntimeError> {
        if !path.exists() {
            return Err(model_manager::ModelManagerError::FileNotFound(path.to_path_buf()).into());
        }

        let model_config = ModelConfig {
            path: path.to_path_buf(),
            context_size: self.config.max_context_tokens,
            gpu_layers: None,
        };

        let mut engine = self.engine.lock().await;
        engine.load_model(path, &model_config).await?;

        // Create a minimal metadata entry
        let metadata = ModelMetadata {
            id: ModelId::from(model_id),
            name: model_id.to_string(),
            path: path.to_path_buf(),
            quantization: model_manager::QuantType::Q4KM,
            size_bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            estimated_ram_bytes: 0,
            context_limit: self.config.max_context_tokens,
            sha256: String::new(),
            last_used: chrono::Utc::now(),
            download_url: None,
            license: "unknown".into(),
            min_ram_bytes: 0,
            tags: vec![],
        };

        *self.loaded_model.lock().await = Some(metadata);
        info!(model = model_id, path = %path.display(), "Model loaded from path");
        Ok(())
    }

    /// Run inference on the loaded model. Returns a receiver for streaming tokens.
    pub async fn run_inference(
        &self,
        prompt: &str,
        params: GenerationParams,
    ) -> Result<(mpsc::Receiver<Token>, CancellationToken), RuntimeError> {
        if self.loaded_model.lock().await.is_none() {
            return Err(RuntimeError::NoModelLoaded);
        }

        let (tx, rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();

        let engine = self.engine.clone();
        let prompt = prompt.to_string();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            let engine = engine.lock().await;
            match engine
                .stream_completion(&prompt, &params, tx, cancel_clone)
                .await
            {
                Ok(stats) => {
                    info!(
                        tokens = stats.tokens_generated,
                        tps = format!("{:.1}", stats.tokens_per_second),
                        "Inference completed"
                    );
                }
                Err(inference_engine::InferenceError::Cancelled) => {
                    info!("Inference cancelled by user");
                }
                Err(e) => {
                    warn!(error = %e, "Inference error");
                }
            }
        });

        Ok((rx, cancel))
    }

    /// Unload the current model.
    pub async fn unload_model(&self) -> Result<(), RuntimeError> {
        let mut engine = self.engine.lock().await;
        engine.unload_model().await?;
        *self.loaded_model.lock().await = None;
        info!("Model unloaded");
        Ok(())
    }

    /// Get a reference to the model registry.
    pub fn registry(&self) -> &InMemoryRegistry {
        &self.registry
    }

    /// Get the memory guard for querying system memory info.
    pub fn memory_guard(&self) -> &WatermarkGuard {
        &self.memory_guard
    }

    /// Get the runtime config.
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Graceful shutdown.
    pub async fn shutdown(&self) -> Result<(), RuntimeError> {
        info!("Runtime shutting down");
        self.unload_model().await.ok(); // Best-effort unload
        Ok(())
    }
}
