use std::sync::Arc;

use device_profiler::{recommend_quantization, SystemProfiler};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use inference_engine::{InferenceBackend, ModelConfig, Token};
use memory_guard::{MemoryEvent, MemoryGuard, WatermarkGuard};
use model_manager::{
    evict_until_within_quota, GenerationParams, InMemoryRegistry, ModelId, ModelMetadata,
    ModelRegistry,
};

const STREAM_ERROR_PREFIX: &str = "__MAI_ERROR__:";

use crate::config::RuntimeConfig;
use crate::error::RuntimeError;

/// The core runtime orchestrator.
pub struct Runtime {
    config: RuntimeConfig,
    engine: Arc<Mutex<Box<dyn InferenceBackend>>>,
    loaded_model: Arc<Mutex<Option<ModelMetadata>>>,
    registry: InMemoryRegistry,
    memory_guard: WatermarkGuard,
    monitor_task: Mutex<Option<JoinHandle<()>>>,
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
        let engine = Arc::new(Mutex::new(backend));
        let loaded_model = Arc::new(Mutex::new(None));
        let monitor_task = if let Some(mut event_rx) = memory_guard.start_monitor(1000) {
            let engine = engine.clone();
            let loaded_model = loaded_model.clone();
            let registry = registry.clone();
            Some(tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    match event {
                        MemoryEvent::Critical { used_pct } => {
                            warn!(
                                used_pct = format!("{:.1}%", used_pct * 100.0),
                                "Critical memory pressure detected"
                            );
                            if let Err(e) =
                                handle_critical_memory_event(&engine, &loaded_model, &registry)
                                    .await
                            {
                                warn!(error = %e, "Failed handling critical memory event");
                            }
                        }
                        MemoryEvent::Warning { used_pct } => {
                            warn!(
                                used_pct = format!("{:.1}%", used_pct * 100.0),
                                "Memory warning"
                            );
                        }
                        MemoryEvent::Normal { used_pct } => {
                            debug!(
                                used_pct = format!("{:.1}%", used_pct * 100.0),
                                "Memory normal"
                            );
                        }
                    }
                }
            }))
        } else {
            None
        };

        info!(
            models_dir = %config.models_dir.display(),
            africa_mode = config.africa_mode,
            "Runtime initialized"
        );

        Ok(Self {
            config,
            engine,
            loaded_model,
            registry,
            memory_guard,
            monitor_task: Mutex::new(monitor_task),
        })
    }

    /// Load a model by ID from the registry.
    pub async fn load_model(&self, model_id: &str) -> Result<(), RuntimeError> {
        let requested_id = ModelId::from(model_id);
        let id = self.select_model_for_device(&requested_id).await?;
        let metadata = self
            .registry
            .get(&id)
            .await?
            .ok_or_else(|| model_manager::ModelManagerError::NotFound(id.clone()))?;

        self.enforce_storage_quota(std::slice::from_ref(&id))
            .await?;

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
            context_size: self.effective_context_size(metadata.context_limit),
            gpu_layers: None,
        };

        let mut engine = self.engine.lock().await;
        engine.load_model(&metadata.path, &model_config).await?;

        // Update state
        *self.loaded_model.lock().await = Some(metadata);
        self.registry.update_last_used(&id).await?;

        info!(model = %id, requested_model = %requested_id, "Model loaded and ready");
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
            context_size: self.effective_context_size(self.config.max_context_tokens),
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
            let tx_error = tx.clone();
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
                    let _ = tx_error
                        .send(Token {
                            text: format!("{STREAM_ERROR_PREFIX}{e}"),
                            id: 0,
                            logprob: None,
                        })
                        .await;
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
        self.memory_guard.stop_monitor();
        if let Some(handle) = self.monitor_task.lock().await.take() {
            let _ = handle.await;
        }
        self.unload_model().await.ok(); // Best-effort unload
        Ok(())
    }

    fn effective_context_size(&self, model_limit: usize) -> usize {
        let mut size = self.config.max_context_tokens.min(model_limit);
        if self.config.africa_mode {
            size = size.min(1024);
        }
        size
    }

    async fn enforce_storage_quota(&self, protected: &[ModelId]) -> Result<(), RuntimeError> {
        let evicted =
            evict_until_within_quota(&self.registry, self.config.max_storage_bytes, protected)
                .await?;
        if !evicted.is_empty() {
            info!(
                count = evicted.len(),
                "Evicted models to satisfy storage quota"
            );
        }
        Ok(())
    }

    async fn select_model_for_device(
        &self,
        requested_id: &ModelId,
    ) -> Result<ModelId, RuntimeError> {
        if !self.config.auto_select_quantization {
            return Ok(requested_id.clone());
        }

        let Some(requested_meta) = self.registry.get(requested_id).await? else {
            return Ok(requested_id.clone());
        };

        let profile = match SystemProfiler::detect() {
            Ok(profile) => profile,
            Err(e) => {
                debug!(
                    error = %e,
                    model = %requested_id,
                    "Device profile detection failed; skipping auto quant selection"
                );
                return Ok(requested_id.clone());
            }
        };

        let recommended = recommend_quantization(&profile);
        if requested_meta.quantization.bits_per_weight() <= recommended.bits_per_weight() {
            return Ok(requested_id.clone());
        }

        let family = model_family_key(&requested_meta.id.0);
        let mut candidates: Vec<ModelMetadata> = self
            .registry
            .list_all()
            .await?
            .into_iter()
            .filter(|m| {
                model_family_key(&m.id.0) == family
                    && m.quantization.bits_per_weight() <= recommended.bits_per_weight()
            })
            .collect();

        candidates.sort_by(|a, b| {
            let qa = a.quantization.bits_per_weight();
            let qb = b.quantization.bits_per_weight();
            qa.partial_cmp(&qb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.last_used.cmp(&b.last_used))
        });

        let Some(selected) = candidates.pop() else {
            return Ok(requested_id.clone());
        };

        if selected.id != *requested_id {
            info!(
                requested = %requested_id,
                selected = %selected.id,
                recommended_quant = %recommended,
                "Auto-selected quantized model variant for this device"
            );
        }

        Ok(selected.id)
    }
}

fn model_family_key(model_id: &str) -> String {
    let parts: Vec<String> = model_id
        .split(['-', '_', '.'])
        .filter(|p| !p.is_empty())
        .map(|p| p.to_ascii_lowercase())
        .collect();

    let mut out = Vec::with_capacity(parts.len());
    let mut idx = 0usize;
    while idx < parts.len() {
        if is_quant_token(&parts[idx]) {
            idx += 1;
            continue;
        }

        // Handles split quant forms like `q4_k_m` / `q5_k_s`.
        if parts[idx].starts_with('q')
            && parts[idx].len() == 2
            && parts
                .get(idx + 1)
                .is_some_and(|next| next == "k" || next == "0")
        {
            idx += 2;
            if parts
                .get(idx)
                .is_some_and(|suffix| suffix == "m" || suffix == "s")
            {
                idx += 1;
            }
            continue;
        }

        out.push(parts[idx].clone());
        idx += 1;
    }

    out.join("-")
}

fn is_quant_token(token: &str) -> bool {
    let normalized = token
        .to_ascii_lowercase()
        .replace(['-', '_'], "")
        .replace("gguf", "");

    if normalized.is_empty() {
        return true;
    }

    matches!(
        normalized.as_str(),
        "q2k" | "q3ks" | "q3km" | "q4km" | "q4ks" | "q5km" | "q5ks" | "q6k" | "q80" | "f16"
    )
}

async fn handle_critical_memory_event(
    engine: &Arc<Mutex<Box<dyn InferenceBackend>>>,
    loaded_model: &Arc<Mutex<Option<ModelMetadata>>>,
    registry: &InMemoryRegistry,
) -> Result<(), RuntimeError> {
    let active_id = loaded_model.lock().await.as_ref().map(|m| m.id.clone());

    if let Some(evicted_id) = evict_one_lru_non_active(registry, active_id.as_ref()).await? {
        warn!(model = %evicted_id, "Evicted cold model due to memory pressure");
    } else if active_id.is_some() {
        warn!("No cold model available to evict; unloading active model");
        let mut guard = engine.lock().await;
        guard.unload_model().await?;
        drop(guard);
        *loaded_model.lock().await = None;
    }

    Ok(())
}

async fn evict_one_lru_non_active(
    registry: &InMemoryRegistry,
    active_model: Option<&ModelId>,
) -> Result<Option<ModelId>, RuntimeError> {
    let mut candidates = registry.list_all().await?;
    candidates.sort_by_key(|m| m.last_used);

    let Some(candidate) = candidates.into_iter().find(|m| active_model != Some(&m.id)) else {
        return Ok(None);
    };

    let model_id = candidate.id.clone();
    let path = registry.remove_with_file(&model_id).await?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(model_manager::ModelManagerError::EvictionFailed(format!(
                "failed to delete {}: {e}",
                path.display()
            ))
            .into());
        }
    }

    Ok(Some(model_id))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use inference_engine::{CompletionStats, InferenceError, ModelConfig};
    use tokio::sync::mpsc;
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct RecordingBackend {
        loaded: bool,
        last_context_size: Arc<Mutex<Option<usize>>>,
        last_loaded_path: Arc<Mutex<Option<PathBuf>>>,
    }

    impl RecordingBackend {
        fn new(
            last_context_size: Arc<Mutex<Option<usize>>>,
            last_loaded_path: Arc<Mutex<Option<PathBuf>>>,
        ) -> Self {
            Self {
                loaded: false,
                last_context_size,
                last_loaded_path,
            }
        }
    }

    #[async_trait]
    impl InferenceBackend for RecordingBackend {
        async fn load_model(
            &mut self,
            path: &Path,
            config: &ModelConfig,
        ) -> Result<(), InferenceError> {
            *self.last_context_size.lock().await = Some(config.context_size);
            *self.last_loaded_path.lock().await = Some(path.to_path_buf());
            self.loaded = true;
            Ok(())
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
            Ok(CompletionStats {
                tokens_generated: 0,
                tokens_per_second: 0.0,
                prompt_tokens: 0,
                total_duration_ms: 0,
            })
        }

        fn memory_usage_bytes(&self) -> u64 {
            0
        }

        fn is_loaded(&self) -> bool {
            self.loaded
        }
    }

    fn test_config(base: &Path) -> RuntimeConfig {
        RuntimeConfig {
            models_dir: base.join("models"),
            cache_dir: base.join("cache"),
            logs_dir: base.join("logs"),
            max_storage_bytes: 10 * 1024 * 1024,
            max_context_tokens: 4096,
            memory_safety_margin_pct: 0.25,
            inference_timeout_secs: 60,
            africa_mode: false,
            auto_select_quantization: false,
        }
    }

    fn model_metadata(
        id: &str,
        path: PathBuf,
        size: u64,
        estimated_ram: u64,
        sha256: String,
        last_used: chrono::DateTime<Utc>,
    ) -> ModelMetadata {
        ModelMetadata {
            id: id.into(),
            name: id.to_string(),
            path,
            quantization: model_manager::QuantType::Q4KM,
            size_bytes: size,
            estimated_ram_bytes: estimated_ram,
            context_limit: 4096,
            sha256,
            last_used,
            download_url: None,
            license: "unknown".into(),
            min_ram_bytes: 0,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn africa_mode_caps_context_to_1024() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut config = test_config(dir.path());
        config.africa_mode = true;

        let context_capture = Arc::new(Mutex::new(None));
        let path_capture = Arc::new(Mutex::new(None));
        let backend = Box::new(RecordingBackend::new(context_capture.clone(), path_capture));
        let runtime = Runtime::new(config, backend).await.unwrap();

        let path = dir.path().join("model.gguf");
        tokio::fs::write(&path, b"dummy").await.unwrap();
        runtime.load_model_from_path(&path, "local").await.unwrap();

        let captured = *context_capture.lock().await;
        assert_eq!(captured, Some(1024));
    }

    #[tokio::test]
    async fn load_model_refuses_oom_candidate() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = test_config(dir.path());
        let backend = Box::new(RecordingBackend::new(
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
        ));
        let runtime = Runtime::new(config, backend).await.unwrap();

        let path = dir.path().join("oom.gguf");
        tokio::fs::write(&path, b"dummy-model").await.unwrap();
        let sha = model_manager::compute_sha256(&path).await.unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        let metadata = model_metadata("oom", path, size, u64::MAX / 2, sha, Utc::now());
        runtime.registry().register(metadata).await.unwrap();

        let err = runtime.load_model("oom").await.unwrap_err();
        assert!(matches!(
            err,
            RuntimeError::Memory(memory_guard::MemoryError::InsufficientMemory { .. })
        ));
    }

    #[tokio::test]
    async fn storage_quota_evicts_lru_before_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut config = test_config(dir.path());
        config.max_storage_bytes = 18;

        let backend = Box::new(RecordingBackend::new(
            Arc::new(Mutex::new(None)),
            Arc::new(Mutex::new(None)),
        ));
        let runtime = Runtime::new(config, backend).await.unwrap();

        let old_path = dir.path().join("old.gguf");
        let new_path = dir.path().join("new.gguf");
        tokio::fs::write(&old_path, vec![0u8; 12]).await.unwrap();
        tokio::fs::write(&new_path, vec![1u8; 12]).await.unwrap();

        let old_meta = model_metadata(
            "old",
            old_path.clone(),
            12,
            1,
            "unused".into(),
            Utc::now() - Duration::days(3),
        );
        let new_meta = model_metadata(
            "new",
            new_path.clone(),
            12,
            1,
            model_manager::compute_sha256(&new_path).await.unwrap(),
            Utc::now(),
        );

        runtime.registry().register(old_meta).await.unwrap();
        runtime.registry().register(new_meta).await.unwrap();

        let total = runtime.memory_guard().total_memory_bytes();
        let free = runtime.memory_guard().free_memory_bytes();
        let used_pct = if total > 0 {
            (total - free) as f64 / total as f64
        } else {
            0.0
        };
        if used_pct > 0.90 {
            eprintln!(
                "Skipping quota eviction test due to critical host memory usage: {:.1}%",
                used_pct * 100.0
            );
            return;
        }

        runtime.load_model("new").await.unwrap();

        assert!(!old_path.exists());
        assert!(runtime
            .registry()
            .get(&ModelId::from("old"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn auto_selects_lighter_quant_variant() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut config = test_config(dir.path());
        config.auto_select_quantization = true;

        let context_capture = Arc::new(Mutex::new(None));
        let path_capture = Arc::new(Mutex::new(None));
        let backend = Box::new(RecordingBackend::new(context_capture, path_capture.clone()));
        let runtime = Runtime::new(config, backend).await.unwrap();

        let f16_path = dir.path().join("phi3-f16.gguf");
        let q4_path = dir.path().join("phi3-q4.gguf");
        tokio::fs::write(&f16_path, b"f16").await.unwrap();
        tokio::fs::write(&q4_path, b"q4").await.unwrap();

        let f16_meta = model_metadata(
            "phi3-f16",
            f16_path.clone(),
            3,
            3,
            model_manager::compute_sha256(&f16_path).await.unwrap(),
            Utc::now() - Duration::hours(3),
        );
        let q4_meta = model_metadata(
            "phi3-q4km",
            q4_path.clone(),
            2,
            2,
            model_manager::compute_sha256(&q4_path).await.unwrap(),
            Utc::now() - Duration::hours(1),
        );

        runtime.registry().register(f16_meta).await.unwrap();
        runtime.registry().register(q4_meta).await.unwrap();

        let total = runtime.memory_guard().total_memory_bytes();
        let free = runtime.memory_guard().free_memory_bytes();
        let used_pct = if total > 0 {
            (total - free) as f64 / total as f64
        } else {
            0.0
        };
        if free == 0 || used_pct > 0.90 {
            eprintln!(
                "Skipping auto-select test due to host memory constraints: free={} used={:.1}%",
                free,
                used_pct * 100.0
            );
            return;
        }

        runtime.load_model("phi3-f16").await.unwrap();

        let loaded = path_capture.lock().await.clone();
        assert_eq!(loaded, Some(q4_path));
    }

    #[test]
    fn strips_quant_tokens_from_family_key() {
        assert_eq!(model_family_key("phi-3-mini-q4_k_m"), "phi-3-mini");
        assert_eq!(model_family_key("tinyllama-q6k.gguf"), "tinyllama");
    }
}
