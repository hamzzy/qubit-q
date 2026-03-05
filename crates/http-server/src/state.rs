use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use runtime_core::Runtime;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

#[derive(Debug, Default)]
pub struct Metrics {
    pub inference_total: AtomicU64,
    pub inference_errors_total: AtomicU64,
    pub active_streams: AtomicU64,
    pub downloads_started_total: AtomicU64,
    pub downloads_completed_total: AtomicU64,
    pub downloads_failed_total: AtomicU64,
    pub downloads_active: AtomicU64,
    pub download_bytes_total: AtomicU64,
}

impl Metrics {
    pub fn inc_inference_total(&self) {
        self.inference_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_inference_errors(&self) {
        self.inference_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_active_streams(&self) {
        self.active_streams.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_active_streams(&self) {
        self.active_streams.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn mark_download_started(&self) {
        self.downloads_started_total.fetch_add(1, Ordering::Relaxed);
        self.downloads_active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mark_download_completed(&self, bytes_downloaded: u64) {
        self.downloads_completed_total
            .fetch_add(1, Ordering::Relaxed);
        self.downloads_active.fetch_sub(1, Ordering::Relaxed);
        self.download_bytes_total
            .fetch_add(bytes_downloaded, Ordering::Relaxed);
    }

    pub fn mark_download_failed(&self) {
        self.downloads_failed_total.fetch_add(1, Ordering::Relaxed);
        self.downloads_active.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSpec {
    pub source_path: Option<PathBuf>,
    pub source_url: Option<String>,
    pub destination_path: PathBuf,
    pub id: String,
    pub name: String,
    pub quant: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadJobSnapshot {
    pub job_id: String,
    pub model_id: String,
    pub model_name: String,
    pub quant: String,
    pub source: String,
    pub destination_path: String,
    pub status: DownloadStatus,
    pub resumed_from_bytes: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress_pct: Option<f64>,
    pub retries: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
struct DownloadJobRecord {
    spec: DownloadSpec,
    snapshot: DownloadJobSnapshot,
}

#[derive(Debug, Default)]
pub struct DownloadTracker {
    next_id: AtomicU64,
    jobs: RwLock<HashMap<String, DownloadJobRecord>>,
    cancels: RwLock<HashMap<String, tokio_util::sync::CancellationToken>>,
}

impl DownloadTracker {
    pub fn create_job(&self, spec: DownloadSpec, source: String) -> DownloadJobSnapshot {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let job_id = format!("dl-{id_num}");
        let now = Utc::now();

        let snapshot = DownloadJobSnapshot {
            job_id: job_id.clone(),
            model_id: spec.id.clone(),
            model_name: spec.name.clone(),
            quant: spec.quant.clone(),
            source,
            destination_path: spec.destination_path.display().to_string(),
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

        if let Ok(mut jobs) = self.jobs.write() {
            jobs.insert(
                job_id,
                DownloadJobRecord {
                    spec,
                    snapshot: snapshot.clone(),
                },
            );
        }
        if let Ok(mut cancels) = self.cancels.write() {
            cancels.insert(
                snapshot.job_id.clone(),
                tokio_util::sync::CancellationToken::new(),
            );
        }

        snapshot
    }

    pub fn mark_running(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.write() {
            if let Some(job) = jobs.get_mut(job_id) {
                job.snapshot.status = DownloadStatus::Running;
                job.snapshot.updated_at = Utc::now();
                job.snapshot.error = None;
            }
        }
    }

    pub fn update_progress(
        &self,
        job_id: &str,
        resumed_from_bytes: u64,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        retries: usize,
    ) {
        if let Ok(mut jobs) = self.jobs.write() {
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

    pub fn mark_succeeded(
        &self,
        job_id: &str,
        resumed_from_bytes: u64,
        downloaded_bytes: u64,
        total_bytes: u64,
        retries: usize,
    ) {
        if let Ok(mut jobs) = self.jobs.write() {
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

    pub fn mark_failed(&self, job_id: &str, error: String) {
        if let Ok(mut jobs) = self.jobs.write() {
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

    pub fn mark_cancelled(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.write() {
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

    pub fn get_snapshot(&self, job_id: &str) -> Option<DownloadJobSnapshot> {
        let jobs = self.jobs.read().ok()?;
        jobs.get(job_id).map(|r| r.snapshot.clone())
    }

    pub fn list_snapshots(&self) -> Vec<DownloadJobSnapshot> {
        let mut out = self
            .jobs
            .read()
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

    pub fn get_spec(&self, job_id: &str) -> Option<DownloadSpec> {
        let jobs = self.jobs.read().ok()?;
        jobs.get(job_id).map(|r| r.spec.clone())
    }

    pub fn cancellation_token(&self, job_id: &str) -> Option<tokio_util::sync::CancellationToken> {
        let cancels = self.cancels.read().ok()?;
        cancels.get(job_id).cloned()
    }

    pub fn cancel_job(&self, job_id: &str) -> Option<DownloadStatus> {
        let status = {
            let jobs = self.jobs.read().ok()?;
            jobs.get(job_id).map(|r| r.snapshot.status)?
        };

        let token = {
            let cancels = self.cancels.read().ok()?;
            cancels.get(job_id).cloned()?
        };
        token.cancel();
        Some(status)
    }

    pub fn remove_job(&self, job_id: &str) -> Option<DownloadSpec> {
        if let Ok(mut cancels) = self.cancels.write() {
            if let Some(token) = cancels.remove(job_id) {
                token.cancel();
            }
        }

        let mut jobs = self.jobs.write().ok()?;
        jobs.remove(job_id).map(|r| r.spec)
    }

    fn clear_cancel(&self, job_id: &str) {
        if let Ok(mut cancels) = self.cancels.write() {
            cancels.remove(job_id);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub runtime: Arc<Runtime>,
    pub api_key: Option<String>,
    pub metrics: Arc<Metrics>,
    pub downloads: Arc<DownloadTracker>,
    /// Limits concurrent inference requests. Only one inference can run at a
    /// time (the engine holds a Mutex internally), so this semaphore gives
    /// callers a clear "busy" error instead of blocking indefinitely.
    pub inference_semaphore: Arc<Semaphore>,
}

impl AppState {
    pub fn new(runtime: Runtime, api_key: Option<String>) -> Self {
        Self {
            runtime: Arc::new(runtime),
            api_key,
            metrics: Arc::new(Metrics::default()),
            downloads: Arc::new(DownloadTracker::default()),
            inference_semaphore: Arc::new(Semaphore::new(1)),
        }
    }
}
