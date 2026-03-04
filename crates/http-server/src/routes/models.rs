use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use model_manager::hub::{HubModelListResponse, HubSearchRequest};
use model_manager::{ModelRegistry, QuantType};
use serde::{Deserialize, Serialize};

use crate::error::HttpServerError;
use crate::state::{AppState, DownloadJobSnapshot, DownloadSpec};

#[derive(Debug, Deserialize)]
pub struct DeleteDownloadQuery {
    #[serde(default)]
    pub delete_file: bool,
}

#[derive(Debug, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub owned_by: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: &'static str,
    pub data: Vec<ModelEntry>,
}

#[derive(Debug, Serialize)]
pub struct CatalogResponse {
    pub models: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    pub source_path: Option<PathBuf>,
    pub source_url: Option<String>,
    pub destination_path: PathBuf,
    pub id: String,
    pub name: String,
    pub quant: String,
}

#[derive(Debug, Serialize)]
pub struct DownloadsResponse {
    pub object: &'static str,
    pub data: Vec<DownloadJobSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct RetryDownloadResponse {
    pub retry_of: String,
    pub job: DownloadJobSnapshot,
}

impl From<DownloadRequest> for DownloadSpec {
    fn from(req: DownloadRequest) -> Self {
        Self {
            source_path: req.source_path,
            source_url: req.source_url,
            destination_path: req.destination_path,
            id: req.id,
            name: req.name,
            quant: req.quant,
        }
    }
}

pub async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<ModelsResponse>, HttpServerError> {
    let models = state
        .runtime
        .registry()
        .list_all()
        .await
        .map_err(|e| HttpServerError::Internal(e.to_string()))?;

    let data = models
        .into_iter()
        .map(|m| ModelEntry {
            id: m.id.to_string(),
            object: "model",
            created: m.last_used.timestamp(),
            owned_by: "mai",
        })
        .collect();

    Ok(Json(ModelsResponse {
        object: "list",
        data,
    }))
}

pub async fn catalog() -> Result<Json<CatalogResponse>, HttpServerError> {
    let path = PathBuf::from("models/catalog.json");
    let data = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| HttpServerError::Internal(format!("failed to read catalog: {e}")))?;
    let models: serde_json::Value = serde_json::from_str(&data)
        .map_err(|e| HttpServerError::Internal(format!("invalid catalog json: {e}")))?;

    Ok(Json(CatalogResponse { models }))
}

pub async fn download_model(
    State(state): State<AppState>,
    Json(req): Json<DownloadRequest>,
) -> Result<(StatusCode, Json<DownloadJobSnapshot>), HttpServerError> {
    let spec: DownloadSpec = req.into();
    validate_download_spec(&spec)?;
    let quantization: QuantType = spec.quant.parse().map_err(HttpServerError::BadRequest)?;
    let source_desc = source_descriptor(&spec)?;

    let snapshot = state
        .downloads
        .create_job(spec.clone(), source_desc.clone());
    state.metrics.mark_download_started();

    let state_clone = state.clone();
    let job_id = snapshot.job_id.clone();
    let cancel = state
        .downloads
        .cancellation_token(&job_id)
        .unwrap_or_else(tokio_util::sync::CancellationToken::new);
    tokio::spawn(async move {
        run_download_job(state_clone, job_id, spec, quantization, source_desc, cancel).await;
    });

    Ok((StatusCode::ACCEPTED, Json(snapshot)))
}

pub async fn list_downloads(
    State(state): State<AppState>,
) -> Result<Json<DownloadsResponse>, HttpServerError> {
    Ok(Json(DownloadsResponse {
        object: "list",
        data: state.downloads.list_snapshots(),
    }))
}

pub async fn download_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<DownloadJobSnapshot>, HttpServerError> {
    let Some(snapshot) = state.downloads.get_snapshot(&job_id) else {
        return Err(HttpServerError::BadRequest(format!(
            "download job '{job_id}' not found"
        )));
    };
    Ok(Json(snapshot))
}

pub async fn retry_download(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<(StatusCode, Json<RetryDownloadResponse>), HttpServerError> {
    let Some(spec) = state.downloads.get_spec(&job_id) else {
        return Err(HttpServerError::BadRequest(format!(
            "download job '{job_id}' not found"
        )));
    };
    validate_download_spec(&spec)?;
    let quantization: QuantType = spec.quant.parse().map_err(HttpServerError::BadRequest)?;
    let source_desc = source_descriptor(&spec)?;

    let snapshot = state
        .downloads
        .create_job(spec.clone(), source_desc.clone());
    state.metrics.mark_download_started();

    let state_clone = state.clone();
    let new_job_id = snapshot.job_id.clone();
    let cancel = state
        .downloads
        .cancellation_token(&new_job_id)
        .unwrap_or_else(tokio_util::sync::CancellationToken::new);
    tokio::spawn(async move {
        run_download_job(
            state_clone,
            new_job_id,
            spec,
            quantization,
            source_desc,
            cancel,
        )
        .await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(RetryDownloadResponse {
            retry_of: job_id,
            job: snapshot,
        }),
    ))
}

pub async fn cancel_download(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, HttpServerError> {
    match state.downloads.cancel_job(&job_id) {
        Some(crate::state::DownloadStatus::Queued | crate::state::DownloadStatus::Running) => {
            Ok(StatusCode::NO_CONTENT)
        }
        Some(
            crate::state::DownloadStatus::Succeeded
            | crate::state::DownloadStatus::Failed
            | crate::state::DownloadStatus::Cancelled,
        ) => Err(HttpServerError::BadRequest(format!(
            "download job '{job_id}' is already finalized"
        ))),
        None => Err(HttpServerError::BadRequest(format!(
            "download job '{job_id}' not found"
        ))),
    }
}

pub async fn delete_download(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<DeleteDownloadQuery>,
) -> Result<StatusCode, HttpServerError> {
    let Some(spec) = state.downloads.remove_job(&job_id) else {
        return Err(HttpServerError::BadRequest(format!(
            "download job '{job_id}' not found"
        )));
    };

    if query.delete_file {
        let _ = tokio::fs::remove_file(&spec.destination_path).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn run_download_job(
    state: AppState,
    job_id: String,
    spec: DownloadSpec,
    quantization: QuantType,
    source_desc: String,
    cancel: tokio_util::sync::CancellationToken,
) {
    if cancel.is_cancelled() {
        state.downloads.mark_cancelled(&job_id);
        state.metrics.mark_download_failed();
        return;
    }

    state.downloads.mark_running(&job_id);

    let transfer = match (&spec.source_url, &spec.source_path) {
        (Some(url), None) => model_manager::download_with_resume_report_and_progress_and_cancel(
            url,
            &spec.destination_path,
            {
                let downloads = state.downloads.clone();
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
            &spec.destination_path,
            {
                let downloads = state.downloads.clone();
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
            state.downloads.mark_cancelled(&job_id);
            state.metrics.mark_download_failed();
            return;
        }
        Err(e) => {
            state.downloads.mark_failed(&job_id, e.to_string());
            state.metrics.mark_download_failed();
            return;
        }
    };

    if cancel.is_cancelled() {
        state.downloads.mark_cancelled(&job_id);
        state.metrics.mark_download_failed();
        return;
    }

    let sha256 = match model_manager::compute_sha256(&spec.destination_path).await {
        Ok(hash) => hash,
        Err(e) => {
            state.downloads.mark_failed(&job_id, e.to_string());
            state.metrics.mark_download_failed();
            return;
        }
    };

    if cancel.is_cancelled() {
        state.downloads.mark_cancelled(&job_id);
        state.metrics.mark_download_failed();
        return;
    }

    let metadata = model_manager::ModelMetadata {
        id: spec.id.as_str().into(),
        name: spec.name.clone(),
        path: spec.destination_path.clone(),
        quantization,
        size_bytes: total_size,
        estimated_ram_bytes: total_size.saturating_mul(2),
        context_limit: state.runtime.config().max_context_tokens,
        sha256,
        last_used: chrono::Utc::now(),
        download_url: Some(source_desc),
        license: "unknown".into(),
        min_ram_bytes: 0,
        tags: vec!["catalog".into()],
    };

    if let Err(e) = state.runtime.registry().register(metadata).await {
        state.downloads.mark_failed(&job_id, e.to_string());
        state.metrics.mark_download_failed();
        return;
    }

    let downloaded_bytes = total_size.saturating_sub(resumed_from);
    state
        .downloads
        .mark_succeeded(&job_id, resumed_from, downloaded_bytes, total_size, retries);
    state.metrics.mark_download_completed(downloaded_bytes);
}

pub async fn hub_search(
    Json(req): Json<HubSearchRequest>,
) -> Result<Json<HubModelListResponse>, HttpServerError> {
    let response = model_manager::hub::search_hf_models(req)
        .await
        .map_err(HttpServerError::Internal)?;
    Ok(Json(response))
}

fn validate_download_spec(spec: &DownloadSpec) -> Result<(), HttpServerError> {
    match (&spec.source_url, &spec.source_path) {
        (Some(_), None) => Ok(()),
        (None, Some(_)) => Ok(()),
        (Some(_), Some(_)) => Err(HttpServerError::BadRequest(
            "provide either source_url or source_path, not both".into(),
        )),
        (None, None) => Err(HttpServerError::BadRequest(
            "missing source: set source_url or source_path".into(),
        )),
    }
}

fn source_descriptor(spec: &DownloadSpec) -> Result<String, HttpServerError> {
    match (&spec.source_url, &spec.source_path) {
        (Some(url), None) => Ok(url.clone()),
        (None, Some(path)) => Ok(format!("file://{}", path.display())),
        _ => Err(HttpServerError::BadRequest(
            "unable to resolve download source".into(),
        )),
    }
}
