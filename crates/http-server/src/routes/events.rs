use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

/// SSE endpoint that pushes download progress and metrics snapshots.
///
/// Clients receive JSON events every second while downloads are active, or
/// every 5 seconds when idle. Event types:
///   - `downloads`: array of download job snapshots
///   - `metrics`: current runtime metrics
pub async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(1))).map(
        move |_| {
            let downloads = state.downloads.list_snapshots();
            let has_active = downloads
                .iter()
                .any(|d| matches!(d.status, crate::state::DownloadStatus::Running | crate::state::DownloadStatus::Queued));

            let metrics = &state.metrics;
            let metrics_json = serde_json::json!({
                "inference_total": metrics.inference_total.load(Ordering::Relaxed),
                "inference_errors_total": metrics.inference_errors_total.load(Ordering::Relaxed),
                "active_streams": metrics.active_streams.load(Ordering::Relaxed),
                "downloads_started_total": metrics.downloads_started_total.load(Ordering::Relaxed),
                "downloads_completed_total": metrics.downloads_completed_total.load(Ordering::Relaxed),
                "downloads_failed_total": metrics.downloads_failed_total.load(Ordering::Relaxed),
                "downloads_active": metrics.downloads_active.load(Ordering::Relaxed),
                "download_bytes_total": metrics.download_bytes_total.load(Ordering::Relaxed),
            });

            let payload = serde_json::json!({
                "downloads": downloads,
                "metrics": metrics_json,
                "has_active_downloads": has_active,
            });

            Ok(Event::default()
                .event("update")
                .data(payload.to_string()))
        },
    );

    Sse::new(stream)
}
