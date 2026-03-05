use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use inference_engine::{InferenceBackend, ModelConfig};
use model_manager::{GenerationParams, QuantType};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::detect::SystemProfiler;
use crate::error::ProfilerError;
use crate::profile::DeviceProfile;
use crate::recommender::recommend_quantization;

const CACHE_VERSION: u32 = 1;
const CACHE_TTL_SECS: i64 = 60 * 60 * 24 * 7; // 7 days
const BENCHMARK_PROMPT: &str = "Write a concise summary of local-first AI runtime design.";
const BENCHMARK_MAX_TOKENS: usize = 128;
const BENCHMARK_CONTEXT: usize = 1024;

/// Result of a model benchmark run, including cache metadata.
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub tokens_per_sec: f32,
    pub cache_hit: bool,
    pub measured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    fingerprint: String,
    tokens_per_sec: f32,
    measured_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BenchmarkCache {
    version: u32,
    entries: Vec<CacheEntry>,
}

/// Trait for device profiling and benchmarking.
#[async_trait]
pub trait DeviceProfilerTrait: Send + Sync {
    async fn profile(&self) -> Result<DeviceProfile, ProfilerError>;
    async fn benchmark_tokens_per_sec(&self, model_path: &Path) -> Result<f32, ProfilerError>;
    fn recommend_quantization(&self, profile: &DeviceProfile) -> QuantType;
}

#[async_trait]
impl DeviceProfilerTrait for SystemProfiler {
    async fn profile(&self) -> Result<DeviceProfile, ProfilerError> {
        Self::detect()
    }

    async fn benchmark_tokens_per_sec(&self, model_path: &Path) -> Result<f32, ProfilerError> {
        let result = benchmark_model_tokens_per_sec(model_path).await?;
        Ok(result.tokens_per_sec)
    }

    fn recommend_quantization(&self, profile: &DeviceProfile) -> QuantType {
        recommend_quantization(profile)
    }
}

/// Benchmark a specific model and cache the resulting tokens/sec.
pub async fn benchmark_model_tokens_per_sec(
    model_path: &Path,
) -> Result<BenchmarkResult, ProfilerError> {
    let fingerprint = model_fingerprint(model_path)?;
    let mut cache = load_cache().unwrap_or_else(|e| {
        warn!(error = %e, "Benchmark cache unavailable; continuing without cache");
        BenchmarkCache {
            version: CACHE_VERSION,
            entries: vec![],
        }
    });
    prune_expired(&mut cache, Utc::now());

    if let Some(hit) = cache.entries.iter().find(|e| e.fingerprint == fingerprint) {
        return Ok(BenchmarkResult {
            tokens_per_sec: hit.tokens_per_sec,
            cache_hit: true,
            measured_at: hit.measured_at,
        });
    }

    let tokens_per_sec = run_live_benchmark(model_path).await?;
    let measured_at = Utc::now();

    cache.entries.retain(|e| e.fingerprint != fingerprint);
    cache.entries.push(CacheEntry {
        fingerprint,
        tokens_per_sec,
        measured_at,
    });
    if let Err(e) = save_cache(&cache) {
        warn!(error = %e, "Failed to persist benchmark cache");
    }

    Ok(BenchmarkResult {
        tokens_per_sec,
        cache_hit: false,
        measured_at,
    })
}

async fn run_live_benchmark(model_path: &Path) -> Result<f32, ProfilerError> {
    let mut backend = create_benchmark_backend()?;
    let config = ModelConfig {
        path: model_path.to_path_buf(),
        context_size: BENCHMARK_CONTEXT,
        gpu_layers: None,
    };

    backend
        .load_model(model_path, &config)
        .await
        .map_err(|e| ProfilerError::BenchmarkFailed(format!("load failed: {e}")))?;

    let params = GenerationParams {
        max_tokens: BENCHMARK_MAX_TOKENS,
        temperature: 0.2,
        top_p: 0.95,
        top_k: 40,
        repeat_penalty: 1.0,
        stop_sequences: vec![],
        seed: Some(42),
        stream: true,
    };

    let (tx, mut rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();

    let started = Instant::now();
    let stats = backend
        .stream_completion(BENCHMARK_PROMPT, &params, tx, cancel)
        .await
        .map_err(|e| ProfilerError::BenchmarkFailed(format!("stream failed: {e}")))?;

    let mut token_count = 0usize;
    while rx.recv().await.is_some() {
        token_count += 1;
    }

    let elapsed = started.elapsed().as_secs_f32().max(0.001);
    let measured_tps = if stats.tokens_per_second.is_finite() && stats.tokens_per_second > 0.0 {
        stats.tokens_per_second
    } else {
        token_count as f32 / elapsed
    };

    backend.unload_model().await.ok();

    info!(
        model = %model_path.display(),
        tokens = token_count,
        elapsed_ms = started.elapsed().as_millis(),
        tps = format!("{measured_tps:.2}"),
        "Benchmark run complete"
    );

    Ok(measured_tps.max(0.1))
}

fn create_benchmark_backend() -> Result<Box<dyn InferenceBackend>, ProfilerError> {
    #[cfg(feature = "llama-backend")]
    {
        let backend = inference_engine::llama_backend::LlamaBackendWrapper::new()
            .map_err(|e| ProfilerError::BenchmarkFailed(e.to_string()))?;
        return Ok(Box::new(backend));
    }

    #[allow(unreachable_code)]
    Err(ProfilerError::UnsupportedPlatform(
        "No benchmark backend enabled (enable llama-backend feature)".into(),
    ))
}

fn model_fingerprint(model_path: &Path) -> Result<String, ProfilerError> {
    let canonical = model_path
        .canonicalize()
        .map_err(|e| ProfilerError::BenchmarkFailed(format!("canonicalize failed: {e}")))?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| ProfilerError::BenchmarkFailed(format!("metadata failed: {e}")))?;

    let modified = metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    Ok(format!(
        "v{}:{}:{}:{}:{}",
        CACHE_VERSION,
        canonical.display(),
        metadata.len(),
        modified,
        backend_kind()
    ))
}

fn backend_kind() -> &'static str {
    if cfg!(feature = "llama-backend") {
        "llama-backend"
    } else {
        "none"
    }
}

fn cache_path() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".mai")
        .join("cache")
        .join("device_benchmark_cache.json")
}

fn load_cache() -> Result<BenchmarkCache, ProfilerError> {
    let path = cache_path();
    let data = match std::fs::read_to_string(&path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BenchmarkCache {
                version: CACHE_VERSION,
                entries: vec![],
            });
        }
        Err(e) => {
            return Err(ProfilerError::BenchmarkFailed(format!(
                "failed reading cache {}: {e}",
                path.display()
            )));
        }
    };

    let mut cache: BenchmarkCache = serde_json::from_str(&data).unwrap_or_default();
    if cache.version != CACHE_VERSION {
        cache.version = CACHE_VERSION;
        cache.entries.clear();
    }

    Ok(cache)
}

fn save_cache(cache: &BenchmarkCache) -> Result<(), ProfilerError> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ProfilerError::BenchmarkFailed(format!(
                "failed creating cache directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    let data = serde_json::to_string_pretty(cache)
        .map_err(|e| ProfilerError::BenchmarkFailed(format!("cache serialize failed: {e}")))?;
    std::fs::write(&path, data).map_err(|e| {
        ProfilerError::BenchmarkFailed(format!("failed writing cache {}: {e}", path.display()))
    })
}

fn prune_expired(cache: &mut BenchmarkCache, now: DateTime<Utc>) {
    cache
        .entries
        .retain(|entry| (now - entry.measured_at).num_seconds() <= CACHE_TTL_SECS);
}
