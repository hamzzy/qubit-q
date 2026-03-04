use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, Utc};
use device_profiler::SystemProfiler;
use inference_engine::InferenceBackend;
use model_manager::{GenerationParams, ModelRegistry, QuantType};
use reqwest::Url;
use runtime_core::{Runtime, RuntimeConfig};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime as TokioRuntime;
use tokio_util::sync::CancellationToken;

const SUCCESS: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_INVALID_UTF8: c_int = -2;
const ERR_RUNTIME: c_int = -3;
const ERR_NOT_FOUND: c_int = -4;
const EMBEDDED_CATALOG_JSON: &str = include_str!("../../../models/catalog.json");
const HF_MODELS_API: &str = "https://huggingface.co/api/models";
const BACKEND_AUTO: &str = "auto";
static LAST_ERROR: OnceLock<Mutex<String>> = OnceLock::new();

fn last_error_cell() -> &'static Mutex<String> {
    LAST_ERROR.get_or_init(|| Mutex::new(String::new()))
}

fn clear_last_error_message() {
    if let Ok(mut slot) = last_error_cell().lock() {
        slot.clear();
    }
}

fn set_last_error_message(message: impl Into<String>) {
    if let Ok(mut slot) = last_error_cell().lock() {
        *slot = message.into();
    }
}

fn set_last_error_and_return(code: c_int, message: impl Into<String>) -> c_int {
    set_last_error_message(message);
    code
}

fn get_last_error_message() -> Option<String> {
    let slot = last_error_cell().lock().ok()?;
    let value = slot.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Token callback type: called for each generated token.
/// A final call with `token = NULL` marks stream completion.
pub type TokenCallback = extern "C" fn(token: *const c_char, user_data: *mut c_void);

#[derive(Debug, Default)]
struct FfiMetrics {
    inference_total: AtomicU64,
    inference_errors_total: AtomicU64,
    active_streams: AtomicU64,
    downloads_started_total: AtomicU64,
    downloads_completed_total: AtomicU64,
    downloads_failed_total: AtomicU64,
    downloads_active: AtomicU64,
    download_bytes_total: AtomicU64,
}

impl FfiMetrics {
    fn mark_inference_started(&self) {
        self.inference_total.fetch_add(1, Ordering::Relaxed);
    }

    fn mark_inference_error(&self) {
        self.inference_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_active_streams(&self) {
        self.active_streams.fetch_add(1, Ordering::Relaxed);
    }

    fn dec_active_streams(&self) {
        self.active_streams.fetch_sub(1, Ordering::Relaxed);
    }

    fn mark_download_started(&self) {
        self.downloads_started_total.fetch_add(1, Ordering::Relaxed);
        self.downloads_active.fetch_add(1, Ordering::Relaxed);
    }

    fn mark_download_completed(&self, bytes_downloaded: u64) {
        self.downloads_completed_total
            .fetch_add(1, Ordering::Relaxed);
        self.downloads_active.fetch_sub(1, Ordering::Relaxed);
        self.download_bytes_total
            .fetch_add(bytes_downloaded, Ordering::Relaxed);
    }

    fn mark_download_failed(&self) {
        self.downloads_failed_total.fetch_add(1, Ordering::Relaxed);
        self.downloads_active.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadRequest {
    #[serde(default)]
    source_path: Option<PathBuf>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    destination_path: PathBuf,
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    quant: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogFile {
    models: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogEntry {
    id: String,
    name: String,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    quantization: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct HubSearchRequest {
    query: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    pipeline_tag: Option<String>,
    author: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    only_gguf: Option<bool>,
    hf_token: Option<String>,
}

impl Default for HubSearchRequest {
    fn default() -> Self {
        Self {
            query: None,
            limit: Some(50),
            cursor: None,
            pipeline_tag: Some("text-generation".to_string()),
            author: None,
            sort: None,
            direction: None,
            only_gguf: Some(true),
            hf_token: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct HuggingFaceModelApi {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "modelId", default)]
    model_id: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    likes: Option<u64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    siblings: Vec<HuggingFaceSibling>,
}

#[derive(Debug, Clone, Deserialize)]
struct HuggingFaceSibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct HubModelFile {
    filename: String,
    size_bytes: Option<u64>,
    download_url: String,
    quantization: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HubModelSummary {
    id: String,
    downloads: u64,
    likes: u64,
    tags: Vec<String>,
    gguf_files: Vec<HubModelFile>,
}

#[derive(Debug, Clone, Serialize)]
struct HubModelListResponse {
    object: &'static str,
    data: Vec<HubModelSummary>,
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DownloadStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadJobSnapshot {
    job_id: String,
    model_id: String,
    model_name: String,
    quant: String,
    source: String,
    destination_path: String,
    status: DownloadStatus,
    resumed_from_bytes: u64,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    progress_pct: Option<f64>,
    retries: usize,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct DownloadJobRecord {
    request: DownloadRequest,
    snapshot: DownloadJobSnapshot,
}

#[derive(Debug, Default)]
struct DownloadStore {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<String, DownloadJobRecord>>,
    cancels: Mutex<HashMap<String, CancellationToken>>,
}

impl DownloadStore {
    fn create_job(&self, request: DownloadRequest, source: String) -> DownloadJobSnapshot {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let job_id = format!("dl-{id_num}");
        let now = Utc::now();

        let snapshot = DownloadJobSnapshot {
            job_id: job_id.clone(),
            model_id: request.id.clone(),
            model_name: request.name.clone(),
            quant: request.quant.clone(),
            source,
            destination_path: request.destination_path.display().to_string(),
            status: DownloadStatus::Queued,
            resumed_from_bytes: 0,
            downloaded_bytes: 0,
            total_bytes: None,
            progress_pct: None,
            retries: 0,
            created_at: now,
            updated_at: now,
            completed_at: None,
            error: None,
        };

        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(
                job_id,
                DownloadJobRecord {
                    request,
                    snapshot: snapshot.clone(),
                },
            );
        }
        if let Ok(mut cancels) = self.cancels.lock() {
            cancels.insert(snapshot.job_id.clone(), CancellationToken::new());
        }

        snapshot
    }

    fn mark_running(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(job) = jobs.get_mut(job_id) {
                job.snapshot.status = DownloadStatus::Running;
                job.snapshot.updated_at = Utc::now();
                job.snapshot.error = None;
            }
        }
    }

    fn update_progress(
        &self,
        job_id: &str,
        resumed_from_bytes: u64,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        retries: usize,
    ) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(job) = jobs.get_mut(job_id) {
                job.snapshot.resumed_from_bytes = resumed_from_bytes;
                job.snapshot.downloaded_bytes = downloaded_bytes;
                job.snapshot.total_bytes = total_bytes;
                job.snapshot.retries = retries;
                job.snapshot.progress_pct = total_bytes.and_then(|total| {
                    if total == 0 {
                        None
                    } else {
                        Some(
                            ((resumed_from_bytes + downloaded_bytes) as f64 / total as f64) * 100.0,
                        )
                    }
                });
                job.snapshot.updated_at = Utc::now();
            }
        }
    }

    fn mark_succeeded(
        &self,
        job_id: &str,
        resumed_from_bytes: u64,
        downloaded_bytes: u64,
        total_bytes: u64,
        retries: usize,
    ) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(job) = jobs.get_mut(job_id) {
                let now = Utc::now();
                job.snapshot.status = DownloadStatus::Succeeded;
                job.snapshot.resumed_from_bytes = resumed_from_bytes;
                job.snapshot.downloaded_bytes = downloaded_bytes;
                job.snapshot.total_bytes = Some(total_bytes);
                job.snapshot.progress_pct = Some(100.0);
                job.snapshot.retries = retries;
                job.snapshot.updated_at = now;
                job.snapshot.completed_at = Some(now);
                job.snapshot.error = None;
            }
        }
        self.clear_cancel(job_id);
    }

    fn mark_failed(&self, job_id: &str, error: String) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(job) = jobs.get_mut(job_id) {
                let now = Utc::now();
                job.snapshot.status = DownloadStatus::Failed;
                job.snapshot.updated_at = now;
                job.snapshot.completed_at = Some(now);
                job.snapshot.error = Some(error);
            }
        }
        self.clear_cancel(job_id);
    }

    fn mark_cancelled(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(job) = jobs.get_mut(job_id) {
                let now = Utc::now();
                job.snapshot.status = DownloadStatus::Cancelled;
                job.snapshot.updated_at = now;
                job.snapshot.completed_at = Some(now);
                job.snapshot.error = Some("cancelled by user".to_string());
            }
        }
        self.clear_cancel(job_id);
    }

    fn get_snapshot(&self, job_id: &str) -> Option<DownloadJobSnapshot> {
        let jobs = self.jobs.lock().ok()?;
        jobs.get(job_id).map(|r| r.snapshot.clone())
    }

    fn list_snapshots(&self) -> Vec<DownloadJobSnapshot> {
        let mut out = self
            .jobs
            .lock()
            .ok()
            .map(|jobs| {
                jobs.values()
                    .map(|record| record.snapshot.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.updated_at));
        out
    }

    fn get_request(&self, job_id: &str) -> Option<DownloadRequest> {
        let jobs = self.jobs.lock().ok()?;
        jobs.get(job_id).map(|r| r.request.clone())
    }

    fn cancel_job(&self, job_id: &str) -> Option<DownloadStatus> {
        let status = {
            let jobs = self.jobs.lock().ok()?;
            jobs.get(job_id).map(|r| r.snapshot.status)?
        };

        let token = {
            let cancels = self.cancels.lock().ok()?;
            cancels.get(job_id).cloned()?
        };
        token.cancel();
        Some(status)
    }

    fn remove_job(&self, job_id: &str) -> Option<DownloadJobRecord> {
        if let Ok(mut cancels) = self.cancels.lock() {
            if let Some(token) = cancels.remove(job_id) {
                token.cancel();
            }
        }
        let mut jobs = self.jobs.lock().ok()?;
        jobs.remove(job_id)
    }

    fn cancellation_token(&self, job_id: &str) -> Option<CancellationToken> {
        let cancels = self.cancels.lock().ok()?;
        cancels.get(job_id).cloned()
    }

    fn clear_cancel(&self, job_id: &str) {
        if let Ok(mut cancels) = self.cancels.lock() {
            cancels.remove(job_id);
        }
    }
}

/// Opaque FFI runtime handle.
pub struct RuntimeHandle {
    tokio_rt: TokioRuntime,
    runtime: Arc<Runtime>,
    completions: Arc<Mutex<HashMap<u64, CancellationToken>>>,
    next_completion_id: AtomicU64,
    downloads: Arc<DownloadStore>,
    metrics: Arc<FfiMetrics>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialRuntimeConfig {
    models_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    logs_dir: Option<PathBuf>,
    max_storage_bytes: Option<u64>,
    max_context_tokens: Option<usize>,
    memory_safety_margin_pct: Option<f32>,
    inference_timeout_secs: Option<u64>,
    africa_mode: Option<bool>,
    auto_select_quantization: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RuntimeInitConfig {
    #[serde(flatten)]
    runtime: PartialRuntimeConfig,
    backend_preference: Option<String>,
}

impl PartialRuntimeConfig {
    fn merge_into(self, mut base: RuntimeConfig) -> RuntimeConfig {
        if let Some(v) = self.models_dir {
            base.models_dir = v;
        }
        if let Some(v) = self.cache_dir {
            base.cache_dir = v;
        }
        if let Some(v) = self.logs_dir {
            base.logs_dir = v;
        }
        if let Some(v) = self.max_storage_bytes {
            base.max_storage_bytes = v;
        }
        if let Some(v) = self.max_context_tokens {
            base.max_context_tokens = v;
        }
        if let Some(v) = self.memory_safety_margin_pct {
            base.memory_safety_margin_pct = v;
        }
        if let Some(v) = self.inference_timeout_secs {
            base.inference_timeout_secs = v;
        }
        if let Some(v) = self.africa_mode {
            base.africa_mode = v;
        }
        if let Some(v) = self.auto_select_quantization {
            base.auto_select_quantization = v;
        }
        base
    }
}

#[derive(Debug, Serialize)]
struct DownloadListResponse {
    object: &'static str,
    data: Vec<DownloadJobSnapshot>,
}

#[derive(Debug, Serialize)]
struct MetricsSnapshot {
    inference_total: u64,
    inference_errors_total: u64,
    active_streams: u64,
    downloads_started_total: u64,
    downloads_completed_total: u64,
    downloads_failed_total: u64,
    downloads_active: u64,
    download_bytes_total: u64,
    ram_total_bytes: u64,
    ram_free_bytes: u64,
}

/// Initialize the runtime. Returns opaque handle or null on failure.
#[no_mangle]
pub extern "C" fn mai_runtime_init(config_json: *const c_char) -> *mut RuntimeHandle {
    clear_last_error_message();

    let (config, backend_preference) = match parse_runtime_config(config_json) {
        Ok(config) => config,
        Err(err) => {
            set_last_error_message(format!("runtime init config error: {err}"));
            return std::ptr::null_mut();
        }
    };
    let backend = match create_backend(backend_preference.as_deref()) {
        Ok(backend) => backend,
        Err(err) => {
            set_last_error_message(err);
            return std::ptr::null_mut();
        }
    };

    let tokio_rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            set_last_error_message(format!("failed to initialize Tokio runtime: {err}"));
            return std::ptr::null_mut();
        }
    };

    let runtime = match tokio_rt.block_on(Runtime::new(config, backend)) {
        Ok(runtime) => runtime,
        Err(err) => {
            set_last_error_message(format!("failed to initialize runtime core: {err}"));
            return std::ptr::null_mut();
        }
    };

    let handle = RuntimeHandle {
        tokio_rt,
        runtime: Arc::new(runtime),
        completions: Arc::new(Mutex::new(HashMap::new())),
        next_completion_id: AtomicU64::new(1),
        downloads: Arc::new(DownloadStore::default()),
        metrics: Arc::new(FfiMetrics::default()),
    };

    clear_last_error_message();
    Box::into_raw(Box::new(handle))
}

/// Free runtime and all resources.
#[no_mangle]
///
/// # Safety
/// `handle` must be a valid pointer returned by `mai_runtime_init`, and must not
/// be used again after this call.
pub unsafe extern "C" fn mai_runtime_destroy(handle: *mut RuntimeHandle) {
    if handle.is_null() {
        return;
    }

    // SAFETY: `handle` was allocated by `Box::into_raw` in `mai_runtime_init`,
    // and we guard against null above. This consumes ownership exactly once.
    let boxed = unsafe { Box::from_raw(handle) };

    if let Ok(mut completions) = boxed.completions.lock() {
        for (_, cancel) in completions.drain() {
            cancel.cancel();
        }
    }

    let _ = boxed.tokio_rt.block_on(boxed.runtime.shutdown());
}

/// Load a model by ID. Returns 0 on success, error code otherwise.
#[no_mangle]
pub extern "C" fn mai_load_model(handle: *mut RuntimeHandle, model_id: *const c_char) -> c_int {
    clear_last_error_message();
    load_model_inner(handle, model_id)
}

fn load_model_inner(handle: *mut RuntimeHandle, model_id: *const c_char) -> c_int {
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    let model_id = match read_c_string(model_id) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match handle
        .tokio_rt
        .block_on(handle.runtime.load_model(&model_id))
    {
        Ok(()) => SUCCESS,
        Err(err) => set_last_error_and_return(
            ERR_RUNTIME,
            format!("failed to load model '{model_id}': {err}"),
        ),
    }
}

/// Unload current model.
#[no_mangle]
pub extern "C" fn mai_unload_model(handle: *mut RuntimeHandle) -> c_int {
    clear_last_error_message();
    unload_model_inner(handle)
}

fn unload_model_inner(handle: *mut RuntimeHandle) -> c_int {
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    match handle.tokio_rt.block_on(handle.runtime.unload_model()) {
        Ok(()) => SUCCESS,
        Err(err) => {
            set_last_error_and_return(ERR_RUNTIME, format!("failed to unload model: {err}"))
        }
    }
}

/// Run chat completion. Calls `callback` for each token.
/// Non-blocking: starts streaming and returns immediately.
#[no_mangle]
pub extern "C" fn mai_chat_completion(
    handle: *mut RuntimeHandle,
    prompt: *const c_char,
    callback: TokenCallback,
    user_data: *mut c_void,
    completion_id: *mut u64,
) -> c_int {
    clear_last_error_message();
    chat_completion_inner(handle, prompt, callback, user_data, completion_id)
}

fn chat_completion_inner(
    handle: *mut RuntimeHandle,
    prompt: *const c_char,
    callback: TokenCallback,
    user_data: *mut c_void,
    completion_id: *mut u64,
) -> c_int {
    if completion_id.is_null() {
        return ERR_NULL_PTR;
    }

    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    let prompt = match read_c_string(prompt) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let id = handle.next_completion_id.fetch_add(1, Ordering::Relaxed);

    // SAFETY: `completion_id` is checked for null above and points to caller-owned writable memory.
    unsafe {
        *completion_id = id;
    }

    handle.metrics.mark_inference_started();

    let runtime = handle.runtime.clone();
    let completions = handle.completions.clone();
    let metrics = handle.metrics.clone();
    let user_data_bits = user_data as usize;

    let start_result = handle.tokio_rt.block_on(async move {
        let params = GenerationParams::default();
        let (mut rx, cancel) = runtime
            .run_inference(&prompt, params)
            .await
            .map_err(|e| e.to_string())?;

        {
            let mut map = completions
                .lock()
                .map_err(|_| "completion lock poisoned".to_string())?;
            map.insert(id, cancel);
        }

        metrics.inc_active_streams();
        tokio::spawn(async move {
            while let Some(token) = rx.recv().await {
                let text = token.text.replace('\0', "");
                if let Ok(c_token) = CString::new(text) {
                    callback(c_token.as_ptr(), user_data_bits as *mut c_void);
                }
            }
            callback(std::ptr::null(), user_data_bits as *mut c_void);
            if let Ok(mut map) = completions.lock() {
                map.remove(&id);
            }
            metrics.dec_active_streams();
        });

        Ok::<(), String>(())
    });

    match start_result {
        Ok(()) => SUCCESS,
        Err(err) => {
            handle.metrics.mark_inference_error();
            set_last_error_and_return(
                ERR_RUNTIME,
                format!("failed to start completion for request {id}: {err}"),
            )
        }
    }
}

/// Cancel an in-flight completion by ID.
#[no_mangle]
pub extern "C" fn mai_cancel_completion(handle: *mut RuntimeHandle, completion_id: u64) -> c_int {
    clear_last_error_message();
    cancel_completion_inner(handle, completion_id)
}

fn cancel_completion_inner(handle: *mut RuntimeHandle, completion_id: u64) -> c_int {
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    let cancel = {
        let mut map = match handle.completions.lock() {
            Ok(map) => map,
            Err(_) => {
                return set_last_error_and_return(ERR_RUNTIME, "completion state lock poisoned")
            }
        };
        map.remove(&completion_id)
    };

    match cancel {
        Some(token) => {
            token.cancel();
            SUCCESS
        }
        None => set_last_error_and_return(
            ERR_NOT_FOUND,
            format!("completion id {completion_id} was not found"),
        ),
    }
}

/// Start background download and return job id via `out_job_id`.
#[no_mangle]
pub extern "C" fn mai_download_start(
    handle: *mut RuntimeHandle,
    request_json: *const c_char,
    out_job_id: *mut *mut c_char,
) -> c_int {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    if out_job_id.is_null() {
        return set_last_error_and_return(ERR_NULL_PTR, "out_job_id pointer is null");
    }

    let request_json = match read_c_string(request_json) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let request: DownloadRequest = match serde_json::from_str(&request_json) {
        Ok(r) => r,
        Err(err) => {
            return set_last_error_and_return(
                ERR_RUNTIME,
                format!("invalid download request JSON: {err}"),
            )
        }
    };

    let (request, source_desc) =
        match resolve_download_request(request, &handle.runtime.config().models_dir) {
            Ok(v) => v,
            Err(err) => {
                return set_last_error_and_return(
                    ERR_RUNTIME,
                    format!("invalid download request: {err}"),
                )
            }
        };

    let quantization: QuantType = match request.quant.parse() {
        Ok(q) => q,
        Err(err) => {
            return set_last_error_and_return(
                ERR_RUNTIME,
                format!("unsupported quantization '{}': {err}", request.quant),
            )
        }
    };

    let snapshot = handle
        .downloads
        .create_job(request.clone(), source_desc.clone());
    if let Err(code) = write_out_c_string(out_job_id, &snapshot.job_id) {
        return code;
    }

    handle.metrics.mark_download_started();

    let runtime = handle.runtime.clone();
    let downloads = handle.downloads.clone();
    let metrics = handle.metrics.clone();
    let job_id = snapshot.job_id;
    let cancel = handle
        .downloads
        .cancellation_token(&job_id)
        .unwrap_or_else(CancellationToken::new);

    handle.tokio_rt.spawn(async move {
        run_download_job(
            runtime,
            downloads,
            metrics,
            job_id,
            request,
            quantization,
            source_desc,
            cancel,
        )
        .await;
    });

    SUCCESS
}

/// Get one download job snapshot as JSON.
#[no_mangle]
pub extern "C" fn mai_download_status_json(
    handle: *mut RuntimeHandle,
    job_id: *const c_char,
) -> *mut c_char {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(_) => return std::ptr::null_mut(),
    };

    let job_id = match read_c_string(job_id) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };

    let Some(snapshot) = handle.downloads.get_snapshot(&job_id) else {
        set_last_error_message(format!("download job '{job_id}' not found"));
        return std::ptr::null_mut();
    };

    json_to_c_string(&snapshot)
}

/// Get all download jobs as JSON list.
#[no_mangle]
pub extern "C" fn mai_download_list_json(handle: *mut RuntimeHandle) -> *mut c_char {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(_) => return std::ptr::null_mut(),
    };

    let payload = DownloadListResponse {
        object: "list",
        data: handle.downloads.list_snapshots(),
    };

    json_to_c_string(&payload)
}

/// Retry a download request by job id, returning a new job id.
#[no_mangle]
pub extern "C" fn mai_download_retry(
    handle: *mut RuntimeHandle,
    job_id: *const c_char,
    out_new_job_id: *mut *mut c_char,
) -> c_int {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    if out_new_job_id.is_null() {
        return set_last_error_and_return(ERR_NULL_PTR, "out_new_job_id pointer is null");
    }

    let job_id = match read_c_string(job_id) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let Some(request) = handle.downloads.get_request(&job_id) else {
        return set_last_error_and_return(
            ERR_NOT_FOUND,
            format!("download job '{job_id}' not found"),
        );
    };

    let (request, source_desc) =
        match resolve_download_request(request, &handle.runtime.config().models_dir) {
            Ok(v) => v,
            Err(err) => {
                return set_last_error_and_return(
                    ERR_RUNTIME,
                    format!("cannot retry download: {err}"),
                )
            }
        };

    let quantization: QuantType = match request.quant.parse() {
        Ok(q) => q,
        Err(err) => {
            return set_last_error_and_return(
                ERR_RUNTIME,
                format!("unsupported quantization '{}': {err}", request.quant),
            )
        }
    };

    let snapshot = handle
        .downloads
        .create_job(request.clone(), source_desc.clone());
    if let Err(code) = write_out_c_string(out_new_job_id, &snapshot.job_id) {
        return code;
    }

    handle.metrics.mark_download_started();

    let runtime = handle.runtime.clone();
    let downloads = handle.downloads.clone();
    let metrics = handle.metrics.clone();
    let new_job_id = snapshot.job_id;
    let cancel = handle
        .downloads
        .cancellation_token(&new_job_id)
        .unwrap_or_else(CancellationToken::new);

    handle.tokio_rt.spawn(async move {
        run_download_job(
            runtime,
            downloads,
            metrics,
            new_job_id,
            request,
            quantization,
            source_desc,
            cancel,
        )
        .await;
    });

    SUCCESS
}

/// Cancel a queued/running download job.
#[no_mangle]
pub extern "C" fn mai_download_cancel(handle: *mut RuntimeHandle, job_id: *const c_char) -> c_int {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    let job_id = match read_c_string(job_id) {
        Ok(v) => v,
        Err(code) => return code,
    };

    match handle.downloads.cancel_job(&job_id) {
        Some(DownloadStatus::Queued | DownloadStatus::Running) => SUCCESS,
        Some(DownloadStatus::Succeeded | DownloadStatus::Failed | DownloadStatus::Cancelled) => {
            set_last_error_and_return(
                ERR_RUNTIME,
                format!("download '{job_id}' is already finalized and cannot be cancelled"),
            )
        }
        None => {
            set_last_error_and_return(ERR_NOT_FOUND, format!("download job '{job_id}' not found"))
        }
    }
}

/// Remove a download job from tracker. If `delete_file` is true, remove destination file too.
#[no_mangle]
pub extern "C" fn mai_download_delete(
    handle: *mut RuntimeHandle,
    job_id: *const c_char,
    delete_file: bool,
) -> c_int {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(code) => return code,
    };

    let job_id = match read_c_string(job_id) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let Some(record) = handle.downloads.remove_job(&job_id) else {
        return set_last_error_and_return(
            ERR_NOT_FOUND,
            format!("download job '{job_id}' not found"),
        );
    };

    if delete_file {
        if let Err(err) = std::fs::remove_file(&record.request.destination_path) {
            return set_last_error_and_return(
                ERR_RUNTIME,
                format!(
                    "failed to remove downloaded file '{}': {err}",
                    record.request.destination_path.display()
                ),
            );
        }
    }

    SUCCESS
}

/// Return runtime metrics as JSON.
#[no_mangle]
pub extern "C" fn mai_metrics_json(handle: *mut RuntimeHandle) -> *mut c_char {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(_) => return std::ptr::null_mut(),
    };

    let (ram_total_bytes, ram_free_bytes) = match SystemProfiler::detect() {
        Ok(profile) => (profile.total_ram_bytes, profile.free_ram_bytes),
        Err(_) => (0, 0),
    };

    let payload = MetricsSnapshot {
        inference_total: handle.metrics.inference_total.load(Ordering::Relaxed),
        inference_errors_total: handle
            .metrics
            .inference_errors_total
            .load(Ordering::Relaxed),
        active_streams: handle.metrics.active_streams.load(Ordering::Relaxed),
        downloads_started_total: handle
            .metrics
            .downloads_started_total
            .load(Ordering::Relaxed),
        downloads_completed_total: handle
            .metrics
            .downloads_completed_total
            .load(Ordering::Relaxed),
        downloads_failed_total: handle
            .metrics
            .downloads_failed_total
            .load(Ordering::Relaxed),
        downloads_active: handle.metrics.downloads_active.load(Ordering::Relaxed),
        download_bytes_total: handle.metrics.download_bytes_total.load(Ordering::Relaxed),
        ram_total_bytes,
        ram_free_bytes,
    };

    json_to_c_string(&payload)
}

/// Return model catalog JSON.
#[no_mangle]
pub extern "C" fn mai_model_catalog_json(handle: *mut RuntimeHandle) -> *mut c_char {
    clear_last_error_message();
    if handle_ref(handle).is_err() {
        return std::ptr::null_mut();
    }

    let data = load_catalog_json();
    match CString::new(data) {
        Ok(cstr) => cstr.into_raw(),
        Err(err) => {
            set_last_error_message(format!("failed to return model catalog JSON: {err}"));
            std::ptr::null_mut()
        }
    }
}

/// Search Hugging Face model hub and return compatible models as JSON.
#[no_mangle]
pub extern "C" fn mai_hub_search_models_json(
    handle: *mut RuntimeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    clear_last_error_message();
    let handle = match handle_ref(handle) {
        Ok(h) => h,
        Err(_) => return std::ptr::null_mut(),
    };

    let request = match parse_hub_search_request(request_json) {
        Ok(req) => req,
        Err(_) => return std::ptr::null_mut(),
    };

    let response = match handle.tokio_rt.block_on(search_hf_models(request)) {
        Ok(data) => data,
        Err(err) => {
            set_last_error_message(format!("huggingface search failed: {err}"));
            return std::ptr::null_mut();
        }
    };

    json_to_c_string(&response)
}

/// Get device profile as JSON string. Caller must free with `mai_free_string`.
#[no_mangle]
pub extern "C" fn mai_device_profile_json(handle: *mut RuntimeHandle) -> *mut c_char {
    clear_last_error_message();
    if handle_ref(handle).is_err() {
        return std::ptr::null_mut();
    }

    let profile = match SystemProfiler::detect() {
        Ok(profile) => profile,
        Err(err) => {
            set_last_error_message(format!("failed to detect device profile: {err}"));
            return std::ptr::null_mut();
        }
    };

    json_to_c_string(&profile)
}

/// Return the last native error message captured by this library.
///
/// Caller must free with `mai_free_string`.
#[no_mangle]
pub extern "C" fn mai_last_error_message() -> *mut c_char {
    let Some(msg) = get_last_error_message() else {
        return std::ptr::null_mut();
    };
    match CString::new(msg) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string returned by this API.
#[no_mangle]
///
/// # Safety
/// `s` must be a pointer returned by this API and must not be freed more than once.
pub unsafe extern "C" fn mai_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }

    // SAFETY: `s` must originate from `CString::into_raw` in this crate.
    let _ = unsafe { CString::from_raw(s) };
}

async fn run_download_job(
    runtime: Arc<Runtime>,
    downloads: Arc<DownloadStore>,
    metrics: Arc<FfiMetrics>,
    job_id: String,
    request: DownloadRequest,
    quantization: QuantType,
    source_desc: String,
    cancel: CancellationToken,
) {
    if cancel.is_cancelled() {
        downloads.mark_cancelled(&job_id);
        metrics.mark_download_failed();
        return;
    }

    downloads.mark_running(&job_id);

    let transfer = match (&request.source_url, &request.source_path) {
        (Some(url), None) => model_manager::download_with_resume_report_and_progress_and_cancel(
            url,
            &request.destination_path,
            {
                let downloads = downloads.clone();
                let job_id = job_id.clone();
                move |progress| {
                    downloads.update_progress(
                        &job_id,
                        progress.resumed_from_bytes,
                        progress.downloaded_bytes,
                        progress.total_bytes,
                        progress.retries,
                    );
                }
            },
            {
                let cancel = cancel.clone();
                move || cancel.is_cancelled()
            },
        )
        .await
        .map(|report| {
            (
                report.resumed_from_bytes,
                report.total_bytes,
                report.retries,
            )
        }),
        (None, Some(path)) => model_manager::resume_copy_file_with_progress_and_cancel(
            path,
            &request.destination_path,
            {
                let downloads = downloads.clone();
                let job_id = job_id.clone();
                move |progress| {
                    downloads.update_progress(
                        &job_id,
                        progress.resumed_from_bytes,
                        progress.downloaded_bytes,
                        progress.total_bytes,
                        progress.retries,
                    );
                }
            },
            {
                let cancel = cancel.clone();
                move || cancel.is_cancelled()
            },
        )
        .await
        .map(|(resumed_from, total_size)| (resumed_from, total_size, 0)),
        _ => Err(model_manager::ModelManagerError::DownloadFailed(
            "invalid download source".into(),
        )),
    };

    let (resumed_from, total_size, retries) = match transfer {
        Ok(result) => result,
        Err(model_manager::ModelManagerError::DownloadCancelled) => {
            downloads.mark_cancelled(&job_id);
            metrics.mark_download_failed();
            return;
        }
        Err(e) => {
            downloads.mark_failed(&job_id, e.to_string());
            metrics.mark_download_failed();
            return;
        }
    };

    if cancel.is_cancelled() {
        downloads.mark_cancelled(&job_id);
        metrics.mark_download_failed();
        return;
    }

    let sha256 = match model_manager::compute_sha256(&request.destination_path).await {
        Ok(hash) => hash,
        Err(e) => {
            downloads.mark_failed(&job_id, e.to_string());
            metrics.mark_download_failed();
            return;
        }
    };

    if cancel.is_cancelled() {
        downloads.mark_cancelled(&job_id);
        metrics.mark_download_failed();
        return;
    }

    let metadata = model_manager::ModelMetadata {
        id: request.id.as_str().into(),
        name: request.name.clone(),
        path: request.destination_path.clone(),
        quantization,
        size_bytes: total_size,
        estimated_ram_bytes: total_size.saturating_mul(2),
        context_limit: runtime.config().max_context_tokens,
        sha256,
        last_used: chrono::Utc::now(),
        download_url: Some(source_desc),
        license: "unknown".into(),
        min_ram_bytes: 0,
        tags: vec!["catalog".into()],
    };

    if let Err(e) = runtime.registry().register(metadata).await {
        downloads.mark_failed(&job_id, e.to_string());
        metrics.mark_download_failed();
        return;
    }

    let downloaded_bytes = total_size.saturating_sub(resumed_from);
    downloads.mark_succeeded(&job_id, resumed_from, downloaded_bytes, total_size, retries);
    metrics.mark_download_completed(downloaded_bytes);
}

fn resolve_download_request(
    mut request: DownloadRequest,
    models_dir: &Path,
) -> Result<(DownloadRequest, String), String> {
    let source_url = request
        .source_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    request.source_url = source_url;

    let source_path = request
        .source_path
        .as_ref()
        .filter(|p| !p.as_os_str().is_empty())
        .cloned();
    request.source_path = source_path;

    if request.id.trim().is_empty() {
        return Err("missing model id".to_string());
    }
    if request.name.trim().is_empty() {
        request.name = request.id.clone();
    }
    if request.quant.trim().is_empty() {
        request.quant = "Q4_K_M".to_string();
    }

    if request.destination_path.as_os_str().is_empty() {
        request.destination_path =
            models_dir.join(format!("{}.gguf", sanitize_model_id(&request.id)));
    }

    if request.source_url.is_some() && request.source_path.is_some() {
        return Err("provide either source_url or source_path".to_string());
    }

    if let Some(url) = request.source_url.clone() {
        return Ok((request, url));
    }

    if let Some(path) = request.source_path.clone() {
        return Ok((request, format!("file://{}", path.display())));
    }

    let model = resolve_catalog_entry(&request.id)
        .ok_or_else(|| format!("missing source and unknown catalog model '{}'", request.id))?;

    let source_url = model
        .download_url
        .clone()
        .ok_or_else(|| format!("catalog model '{}' has no download_url", request.id))?;

    if request.name.trim().is_empty() {
        request.name = model.name.clone();
    }

    if request.quant.trim().is_empty() {
        request.quant = model
            .quantization
            .clone()
            .unwrap_or_else(|| "Q4KM".to_string());
    }

    request.source_url = Some(source_url.clone());
    Ok((request, source_url))
}

fn resolve_catalog_entry(model_id: &str) -> Option<CatalogEntry> {
    let parsed = serde_json::from_str::<CatalogFile>(&load_catalog_json()).ok()?;
    parsed.models.into_iter().find(|m| m.id == model_id)
}

fn sanitize_model_id(model_id: &str) -> String {
    model_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn load_catalog_json() -> String {
    if let Ok(path) = std::env::var("MAI_MODEL_CATALOG_PATH") {
        if let Ok(data) = std::fs::read_to_string(path) {
            return data;
        }
    }

    if let Ok(data) = std::fs::read_to_string("models/catalog.json") {
        return data;
    }

    if let Ok(data) = std::fs::read_to_string("../../models/catalog.json") {
        return data;
    }

    EMBEDDED_CATALOG_JSON.to_string()
}

fn parse_hub_search_request(request_json: *const c_char) -> Result<HubSearchRequest, c_int> {
    if request_json.is_null() {
        return Ok(HubSearchRequest::default());
    }

    let json = read_c_string(request_json)?;
    if json.trim().is_empty() {
        return Ok(HubSearchRequest::default());
    }

    let mut request: HubSearchRequest = serde_json::from_str(&json).map_err(|e| {
        set_last_error_message(format!("invalid hub search request JSON: {e}"));
        ERR_RUNTIME
    })?;
    if request.limit.is_none() {
        request.limit = HubSearchRequest::default().limit;
    }
    if request.only_gguf.is_none() {
        request.only_gguf = HubSearchRequest::default().only_gguf;
    }
    if request.pipeline_tag.is_none() && request.query.is_none() {
        request.pipeline_tag = HubSearchRequest::default().pipeline_tag;
    }
    Ok(request)
}

async fn search_hf_models(request: HubSearchRequest) -> Result<HubModelListResponse, String> {
    let mut url = Url::parse(HF_MODELS_API).map_err(|e| e.to_string())?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("full", "true");

        let limit = request.limit.unwrap_or(50).clamp(1, 200);
        qp.append_pair("limit", &limit.to_string());

        if let Some(query) = request
            .query
            .as_ref()
            .map(|q| q.trim())
            .filter(|q| !q.is_empty())
        {
            qp.append_pair("search", query);
        }
        if let Some(pipeline_tag) = request
            .pipeline_tag
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("pipeline_tag", pipeline_tag);
        }
        if let Some(author) = request
            .author
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("author", author);
        }
        if let Some(sort) = request
            .sort
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("sort", sort);
        }
        if let Some(direction) = request
            .direction
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("direction", direction);
        }
        if let Some(cursor) = request
            .cursor
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            qp.append_pair("cursor", cursor);
        }
    }

    let client = reqwest::Client::new();
    let mut http = client.get(url);
    if let Some(token) = request
        .hf_token
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        http = http.bearer_auth(token);
    }

    let response = http.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("huggingface api returned {}", response.status()));
    }

    let next_cursor = parse_hf_next_cursor(
        response
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|v| v.to_str().ok()),
    );

    let raw: Vec<HuggingFaceModelApi> = response.json().await.map_err(|e| e.to_string())?;
    let mut models = raw
        .into_iter()
        .filter_map(map_hf_model_summary)
        .collect::<Vec<_>>();

    if request.only_gguf.unwrap_or(true) {
        models.retain(|m| !m.gguf_files.is_empty());
    }

    Ok(HubModelListResponse {
        object: "list",
        data: models,
        next_cursor,
    })
}

fn map_hf_model_summary(raw: HuggingFaceModelApi) -> Option<HubModelSummary> {
    let model_id = raw.id.or(raw.model_id)?;
    let gguf_files = raw
        .siblings
        .into_iter()
        .filter(|s| is_gguf_file(&s.rfilename))
        .map(|s| HubModelFile {
            download_url: format!(
                "https://huggingface.co/{}/resolve/main/{}",
                model_id, s.rfilename
            ),
            quantization: infer_quantization_from_filename(&s.rfilename),
            filename: s.rfilename,
            size_bytes: s.size,
        })
        .collect::<Vec<_>>();

    Some(HubModelSummary {
        id: model_id,
        downloads: raw.downloads.unwrap_or(0),
        likes: raw.likes.unwrap_or(0),
        tags: raw.tags,
        gguf_files,
    })
}

fn is_gguf_file(filename: &str) -> bool {
    filename.to_ascii_lowercase().ends_with(".gguf")
}

fn infer_quantization_from_filename(filename: &str) -> Option<String> {
    let upper = filename.to_ascii_uppercase();
    let known = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_1", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_1", "Q4_0",
        "Q3_K_M", "Q3_K_S", "Q2_K",
    ];
    known
        .iter()
        .find(|pattern| upper.contains(**pattern))
        .map(|s| (*s).to_string())
}

fn parse_hf_next_cursor(link_header: Option<&str>) -> Option<String> {
    let link_header = link_header?;
    for part in link_header.split(',') {
        if !part.contains("rel=\"next\"") {
            continue;
        }
        let start = part.find('<')?;
        let end = part.find('>')?;
        let url = &part[start + 1..end];
        let parsed = Url::parse(url).ok()?;
        for (key, value) in parsed.query_pairs() {
            if key == "cursor" {
                return Some(value.into_owned());
            }
        }
    }
    None
}

fn parse_runtime_config(
    config_json: *const c_char,
) -> Result<(RuntimeConfig, Option<String>), String> {
    if config_json.is_null() {
        return Ok((RuntimeConfig::default(), None));
    }

    let json =
        read_c_string(config_json).map_err(|_| "invalid runtime config pointer".to_string())?;
    if json.trim().is_empty() {
        return Ok((RuntimeConfig::default(), None));
    }

    let init: RuntimeInitConfig =
        serde_json::from_str(&json).map_err(|e| format!("invalid runtime config JSON: {e}"))?;
    Ok((
        init.runtime.merge_into(RuntimeConfig::default()),
        init.backend_preference
            .map(|s| s.trim().to_ascii_lowercase()),
    ))
}

fn read_c_string(ptr: *const c_char) -> Result<String, c_int> {
    if ptr.is_null() {
        set_last_error_message("received null C string pointer");
        return Err(ERR_NULL_PTR);
    }

    // SAFETY: pointer validity and NUL-termination are required by the C ABI contract.
    let c_str = unsafe { CStr::from_ptr(ptr) };
    let s = c_str.to_str().map_err(|e| {
        set_last_error_message(format!("received invalid UTF-8 string: {e}"));
        ERR_INVALID_UTF8
    })?;
    Ok(s.to_owned())
}

fn write_out_c_string(out: *mut *mut c_char, value: &str) -> Result<(), c_int> {
    let cstr = CString::new(value).map_err(|_| {
        set_last_error_message("string contains embedded NUL byte");
        ERR_INVALID_UTF8
    })?;
    // SAFETY: caller provides a writable pointer for output string pointer.
    unsafe {
        *out = cstr.into_raw();
    }
    Ok(())
}

fn json_to_c_string<T: Serialize>(value: &T) -> *mut c_char {
    let json = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(err) => {
            set_last_error_message(format!("failed to serialize JSON response: {err}"));
            return std::ptr::null_mut();
        }
    };
    match CString::new(json) {
        Ok(cstr) => cstr.into_raw(),
        Err(err) => {
            set_last_error_message(format!("failed to encode JSON C string: {err}"));
            std::ptr::null_mut()
        }
    }
}

fn handle_ref(handle: *mut RuntimeHandle) -> Result<&'static RuntimeHandle, c_int> {
    if handle.is_null() {
        set_last_error_message("runtime handle is null");
        return Err(ERR_NULL_PTR);
    }

    // SAFETY: handle comes from `mai_runtime_init` and remains valid until `mai_runtime_destroy`.
    let handle_ref = unsafe { &*handle };
    Ok(handle_ref)
}

fn create_backend(preferred_backend: Option<&str>) -> Result<Box<dyn InferenceBackend>, String> {
    let requested = preferred_backend
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            std::env::var("MAI_BACKEND")
                .ok()
                .map(|v| v.trim().to_ascii_lowercase())
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| BACKEND_AUTO.to_string());

    match requested.as_str() {
        BACKEND_AUTO => create_backend_auto(),
        "mlx" => create_backend_mlx(),
        "llama" => create_backend_llama(),
        "mock" => create_backend_mock(),
        other => Err(format!(
            "Unknown backend '{other}'. Supported values: auto, mlx, llama, mock"
        )),
    }
}

fn create_backend_auto() -> Result<Box<dyn InferenceBackend>, String> {
    #[cfg(all(feature = "mlx-backend", target_os = "ios"))]
    {
        return create_backend_mlx();
    }

    #[cfg(feature = "llama-backend")]
    {
        return create_backend_llama();
    }

    #[cfg(feature = "mock-backend")]
    {
        return create_backend_mock();
    }

    #[allow(unreachable_code)]
    Err("No inference backend feature enabled".to_string())
}

#[cfg(feature = "mlx-backend")]
fn create_backend_mlx() -> Result<Box<dyn InferenceBackend>, String> {
    Ok(Box::new(inference_engine::mlx_backend::MlxBackend::new()))
}

#[cfg(not(feature = "mlx-backend"))]
fn create_backend_mlx() -> Result<Box<dyn InferenceBackend>, String> {
    Err("MLX backend requested but crate was built without 'mlx-backend'".to_string())
}

#[cfg(feature = "llama-backend")]
fn create_backend_llama() -> Result<Box<dyn InferenceBackend>, String> {
    let backend =
        inference_engine::llama_backend::LlamaBackendWrapper::new().map_err(|e| e.to_string())?;
    Ok(Box::new(backend))
}

#[cfg(not(feature = "llama-backend"))]
fn create_backend_llama() -> Result<Box<dyn InferenceBackend>, String> {
    Err("llama backend requested but crate was built without 'llama-backend'".to_string())
}

#[cfg(feature = "mock-backend")]
fn create_backend_mock() -> Result<Box<dyn InferenceBackend>, String> {
    Ok(Box::new(inference_engine::mock_backend::MockBackend::new()))
}

#[cfg(not(feature = "mock-backend"))]
fn create_backend_mock() -> Result<Box<dyn InferenceBackend>, String> {
    Err("mock backend requested but crate was built without 'mock-backend'".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_with_temp_dirs() -> *mut RuntimeHandle {
        let dir = tempfile::tempdir().unwrap();
        let json = serde_json::json!({
            "models_dir": dir.path().join("models"),
            "cache_dir": dir.path().join("cache"),
            "logs_dir": dir.path().join("logs"),
        })
        .to_string();
        let c = CString::new(json).unwrap();
        let handle = mai_runtime_init(c.as_ptr());
        std::mem::forget(dir);
        handle
    }

    #[test]
    fn ffi_init_and_destroy_with_default_config() {
        let handle = mai_runtime_init(std::ptr::null());
        assert!(!handle.is_null());
        // SAFETY: `handle` was returned by `mai_runtime_init`.
        unsafe { mai_runtime_destroy(handle) };
    }

    #[test]
    fn ffi_device_profile_returns_string() {
        let handle = mai_runtime_init(std::ptr::null());
        assert!(!handle.is_null());

        let json_ptr = mai_device_profile_json(handle);
        assert!(!json_ptr.is_null());

        // SAFETY: pointer comes from `mai_device_profile_json` and is valid until freed.
        let json = unsafe { CStr::from_ptr(json_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        assert!(json.contains("total_ram_bytes"));

        // SAFETY: pointer came from this API.
        unsafe { mai_free_string(json_ptr) };
        // SAFETY: `handle` was returned by `mai_runtime_init`.
        unsafe { mai_runtime_destroy(handle) };
    }

    #[test]
    fn ffi_rejects_null_handle_for_load() {
        let id = CString::new("phi-3-mini-q4").unwrap();
        let code = mai_load_model(std::ptr::null_mut(), id.as_ptr());
        assert_eq!(code, ERR_NULL_PTR);
    }

    #[test]
    fn hf_quantization_is_detected_from_filename() {
        let q = infer_quantization_from_filename("mistral-7b-instruct.Q4_K_M.gguf");
        assert_eq!(q.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn hf_link_header_next_cursor_is_parsed() {
        let link = r#"<https://huggingface.co/api/models?cursor=abc123>; rel="next", <https://huggingface.co/api/models>; rel="prev""#;
        let cursor = parse_hf_next_cursor(Some(link));
        assert_eq!(cursor.as_deref(), Some("abc123"));
    }

    #[test]
    fn hf_model_mapping_extracts_gguf_files() {
        let raw = HuggingFaceModelApi {
            id: Some("TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF".to_string()),
            model_id: None,
            downloads: Some(42),
            likes: Some(7),
            tags: vec!["gguf".to_string()],
            siblings: vec![
                HuggingFaceSibling {
                    rfilename: "README.md".to_string(),
                    size: Some(100),
                },
                HuggingFaceSibling {
                    rfilename: "tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf".to_string(),
                    size: Some(1234),
                },
            ],
        };

        let mapped = map_hf_model_summary(raw).unwrap();
        assert_eq!(mapped.id, "TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF");
        assert_eq!(mapped.gguf_files.len(), 1);
        assert_eq!(mapped.gguf_files[0].quantization.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn ffi_download_rejects_invalid_request() {
        let handle = init_with_temp_dirs();
        assert!(!handle.is_null());

        let req = CString::new(
            r#"{"id":"m","name":"M","quant":"Q4KM","destination_path":"/tmp/x.gguf"}"#,
        )
        .unwrap();
        let mut out: *mut c_char = std::ptr::null_mut();
        let code = mai_download_start(handle, req.as_ptr(), &mut out as *mut *mut c_char);
        assert_eq!(code, ERR_RUNTIME);
        assert!(out.is_null());

        // SAFETY: `handle` was returned by `mai_runtime_init`.
        unsafe { mai_runtime_destroy(handle) };
    }

    #[test]
    fn ffi_download_resolves_catalog_source() {
        let dir = tempfile::tempdir().unwrap();
        let expected_dest = dir.path().join("models").join("tinyllama-1b-q4.gguf");

        let config = serde_json::json!({
            "models_dir": dir.path().join("models"),
            "cache_dir": dir.path().join("cache"),
            "logs_dir": dir.path().join("logs"),
        })
        .to_string();
        let config_c = CString::new(config).unwrap();
        let handle = mai_runtime_init(config_c.as_ptr());
        assert!(!handle.is_null());

        let request = serde_json::json!({
            "id": "tinyllama-1b-q4",
            "name": "TinyLlama 1.1B (Q4_K_M)",
            "quant": "Q4KM",
        })
        .to_string();

        let req_c = CString::new(request).unwrap();
        let mut job_ptr: *mut c_char = std::ptr::null_mut();
        let code = mai_download_start(handle, req_c.as_ptr(), &mut job_ptr as *mut *mut c_char);
        assert_eq!(code, SUCCESS);
        assert!(!job_ptr.is_null());
        // SAFETY: pointer comes from this API.
        let job_id = unsafe { CStr::from_ptr(job_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        // SAFETY: pointer came from this API.
        unsafe { mai_free_string(job_ptr) };

        let id_c = CString::new(job_id).unwrap();
        let status_ptr = mai_download_status_json(handle, id_c.as_ptr());
        assert!(!status_ptr.is_null());
        // SAFETY: pointer came from this API.
        let status_json = unsafe { CStr::from_ptr(status_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        assert!(status_json.contains(expected_dest.to_string_lossy().as_ref()));
        // SAFETY: pointer came from this API.
        unsafe { mai_free_string(status_ptr) };

        // SAFETY: `handle` was returned by `mai_runtime_init`.
        unsafe { mai_runtime_destroy(handle) };
    }

    #[test]
    fn ffi_download_lifecycle_local_copy() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src.gguf");
        let dest = dir.path().join("dst.gguf");
        std::fs::write(&source, vec![1u8; 2048]).unwrap();

        let config = serde_json::json!({
            "models_dir": dir.path().join("models"),
            "cache_dir": dir.path().join("cache"),
            "logs_dir": dir.path().join("logs"),
        })
        .to_string();
        let config_c = CString::new(config).unwrap();
        let handle = mai_runtime_init(config_c.as_ptr());
        assert!(!handle.is_null());

        let request = serde_json::json!({
            "id": "tiny-q4",
            "name": "Tiny Q4",
            "quant": "Q4KM",
            "source_path": source,
            "source_url": serde_json::Value::Null,
            "destination_path": dest,
        })
        .to_string();
        let req_c = CString::new(request).unwrap();
        let mut job_ptr: *mut c_char = std::ptr::null_mut();
        let code = mai_download_start(handle, req_c.as_ptr(), &mut job_ptr as *mut *mut c_char);
        assert_eq!(code, SUCCESS);
        assert!(!job_ptr.is_null());

        // SAFETY: pointer comes from this API.
        let job_id = unsafe { CStr::from_ptr(job_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        // SAFETY: pointer came from this API.
        unsafe { mai_free_string(job_ptr) };

        let mut succeeded = false;
        for _ in 0..60 {
            let id_c = CString::new(job_id.clone()).unwrap();
            let status_ptr = mai_download_status_json(handle, id_c.as_ptr());
            if status_ptr.is_null() {
                std::thread::sleep(std::time::Duration::from_millis(25));
                continue;
            }
            // SAFETY: pointer came from this API.
            let status_json = unsafe { CStr::from_ptr(status_ptr) }
                .to_str()
                .unwrap()
                .to_string();
            // SAFETY: pointer came from this API.
            unsafe { mai_free_string(status_ptr) };

            if status_json.contains("\"status\":\"succeeded\"") {
                succeeded = true;
                break;
            }
            if status_json.contains("\"status\":\"failed\"") {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        assert!(succeeded);

        let metrics_ptr = mai_metrics_json(handle);
        assert!(!metrics_ptr.is_null());
        // SAFETY: pointer came from this API.
        let metrics_json = unsafe { CStr::from_ptr(metrics_ptr) }
            .to_str()
            .unwrap()
            .to_string();
        assert!(metrics_json.contains("downloads_completed_total"));
        // SAFETY: pointer came from this API.
        unsafe { mai_free_string(metrics_ptr) };

        // SAFETY: `handle` was returned by `mai_runtime_init`.
        unsafe { mai_runtime_destroy(handle) };
    }
}
