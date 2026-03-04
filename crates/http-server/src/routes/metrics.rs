use std::sync::atomic::Ordering;

use axum::extract::State;
use memory_guard::MemoryGuard;

use crate::state::AppState;

pub async fn prometheus_metrics(State(state): State<AppState>) -> String {
    let total = state.metrics.inference_total.load(Ordering::Relaxed);
    let errors = state.metrics.inference_errors_total.load(Ordering::Relaxed);
    let active = state.metrics.active_streams.load(Ordering::Relaxed);
    let downloads_started = state
        .metrics
        .downloads_started_total
        .load(Ordering::Relaxed);
    let downloads_completed = state
        .metrics
        .downloads_completed_total
        .load(Ordering::Relaxed);
    let downloads_failed = state.metrics.downloads_failed_total.load(Ordering::Relaxed);
    let downloads_active = state.metrics.downloads_active.load(Ordering::Relaxed);
    let download_bytes = state.metrics.download_bytes_total.load(Ordering::Relaxed);
    let ram_total = state.runtime.memory_guard().total_memory_bytes();
    let ram_free = state.runtime.memory_guard().free_memory_bytes();

    format!(
        "# HELP mai_inference_total Total completions run\n\
         # TYPE mai_inference_total counter\n\
         mai_inference_total {}\n\
         # HELP mai_inference_errors_total Total inference errors\n\
         # TYPE mai_inference_errors_total counter\n\
         mai_inference_errors_total {}\n\
         # HELP mai_active_streams In-flight streaming completions\n\
         # TYPE mai_active_streams gauge\n\
         mai_active_streams {}\n\
         # HELP mai_downloads_started_total Total downloads scheduled\n\
         # TYPE mai_downloads_started_total counter\n\
         mai_downloads_started_total {}\n\
         # HELP mai_downloads_completed_total Total downloads completed\n\
         # TYPE mai_downloads_completed_total counter\n\
         mai_downloads_completed_total {}\n\
         # HELP mai_downloads_failed_total Total downloads failed\n\
         # TYPE mai_downloads_failed_total counter\n\
         mai_downloads_failed_total {}\n\
         # HELP mai_downloads_active Active download jobs\n\
         # TYPE mai_downloads_active gauge\n\
         mai_downloads_active {}\n\
         # HELP mai_download_bytes_total Total downloaded bytes (excluding resumed bytes)\n\
         # TYPE mai_download_bytes_total counter\n\
         mai_download_bytes_total {}\n\
         # HELP mai_ram_total_bytes System RAM total bytes\n\
         # TYPE mai_ram_total_bytes gauge\n\
         mai_ram_total_bytes {}\n\
         # HELP mai_ram_free_bytes System RAM free bytes\n\
         # TYPE mai_ram_free_bytes gauge\n\
         mai_ram_free_bytes {}\n",
        total,
        errors,
        active,
        downloads_started,
        downloads_completed,
        downloads_failed,
        downloads_active,
        download_bytes,
        ram_total,
        ram_free
    )
}
