use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use tower_http::trace::TraceLayer;
use tracing::info;

use inference_engine::InferenceBackend;
use runtime_core::{Runtime, RuntimeConfig};

use crate::config::ServerConfig;
use crate::error::HttpServerError;
use crate::middleware::auth::auth_middleware;
use crate::routes;
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/models", get(routes::models::list_models))
        .route("/v1/models/catalog", get(routes::models::catalog))
        .route("/v1/models/download", post(routes::models::download_model))
        .route("/v1/models/downloads", get(routes::models::list_downloads))
        .route(
            "/v1/models/downloads/:job_id",
            get(routes::models::download_status),
        )
        .route(
            "/v1/models/downloads/:job_id/retry",
            post(routes::models::retry_download),
        )
        .route(
            "/v1/models/downloads/:job_id/cancel",
            post(routes::models::cancel_download),
        )
        .route(
            "/v1/models/downloads/:job_id",
            axum::routing::delete(routes::models::delete_download),
        )
        .route("/v1/models/hub/search", post(routes::models::hub_search))
        .route("/v1/chat/completions", post(routes::chat::chat_completion))
        .route("/v1/embeddings", post(routes::embeddings::create_embedding))
        .route("/ui/models", get(routes::ui::models_page))
        .route("/health", get(routes::health::health_check))
        .route("/metrics", get(routes::metrics::prometheus_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn run_server(
    config: ServerConfig,
    runtime_config: RuntimeConfig,
) -> Result<(), HttpServerError> {
    if config.tls_cert_path.is_some() ^ config.tls_key_path.is_some() {
        return Err(HttpServerError::BadRequest(
            "TLS requires both cert and key paths".into(),
        ));
    }

    let backend = create_backend().map_err(HttpServerError::Internal)?;
    let runtime = Runtime::new(runtime_config, backend)
        .await
        .map_err(|e| HttpServerError::Internal(e.to_string()))?;
    let state = AppState::new(runtime, config.api_key.clone());

    let app = create_router(state);
    let addr = config.socket_addr();

    if let (Some(cert), Some(key)) = (&config.tls_cert_path, &config.tls_key_path) {
        let tls = RustlsConfig::from_pem_file(cert, key)
            .await
            .map_err(|e| HttpServerError::Internal(format!("invalid TLS config: {e}")))?;
        info!(
            addr = %addr,
            lan_mode = config.lan_mode,
            tls = true,
            cert = %cert.display(),
            "HTTP server starting"
        );
        axum_server::bind_rustls(addr, tls)
            .serve(app.into_make_service())
            .await
            .map_err(|e| HttpServerError::Internal(e.to_string()))
    } else {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| HttpServerError::Internal(e.to_string()))?;

        info!(
            addr = %addr,
            lan_mode = config.lan_mode,
            tls = false,
            "HTTP server starting"
        );

        axum::serve(listener, app)
            .await
            .map_err(|e| HttpServerError::Internal(e.to_string()))
    }
}

fn create_backend() -> Result<Box<dyn InferenceBackend>, String> {
    let requested = std::env::var("MAI_BACKEND")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "auto".to_string());

    match requested.as_str() {
        "auto" => create_backend_auto(),
        "mlx" => create_backend_mlx(),
        "llama" => create_backend_llama(),
        "mock" => create_backend_mock(),
        other => Err(format!(
            "Unknown MAI_BACKEND='{other}'. Supported values: auto, mlx, llama, mock"
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
    Err("No inference backend enabled. Build with mock-backend|llama-backend|mlx-backend".into())
}

#[cfg(feature = "mlx-backend")]
fn create_backend_mlx() -> Result<Box<dyn InferenceBackend>, String> {
    Ok(Box::new(inference_engine::mlx_backend::MlxBackend::new()))
}

#[cfg(not(feature = "mlx-backend"))]
fn create_backend_mlx() -> Result<Box<dyn InferenceBackend>, String> {
    Err("MLX backend requested but this binary was built without 'mlx-backend'".into())
}

#[cfg(feature = "llama-backend")]
fn create_backend_llama() -> Result<Box<dyn InferenceBackend>, String> {
    let backend =
        inference_engine::llama_backend::LlamaBackendWrapper::new().map_err(|e| e.to_string())?;
    Ok(Box::new(backend))
}

#[cfg(not(feature = "llama-backend"))]
fn create_backend_llama() -> Result<Box<dyn InferenceBackend>, String> {
    Err("llama backend requested but this binary was built without 'llama-backend'".into())
}

#[cfg(feature = "mock-backend")]
fn create_backend_mock() -> Result<Box<dyn InferenceBackend>, String> {
    Ok(Box::new(inference_engine::mock_backend::MockBackend::new()))
}

#[cfg(not(feature = "mock-backend"))]
fn create_backend_mock() -> Result<Box<dyn InferenceBackend>, String> {
    Err("mock backend requested but this binary was built without 'mock-backend'".into())
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Method, Request, StatusCode};
    use model_manager::{ModelId, ModelMetadata, ModelRegistry, QuantType};
    use runtime_core::RuntimeConfig;
    use tower::ServiceExt;

    use super::*;

    fn test_runtime_config(base: &std::path::Path) -> RuntimeConfig {
        RuntimeConfig {
            models_dir: base.join("models"),
            cache_dir: base.join("cache"),
            logs_dir: base.join("logs"),
            max_storage_bytes: 10 * 1024 * 1024,
            max_context_tokens: 2048,
            memory_safety_margin_pct: 0.25,
            inference_timeout_secs: 60,
            africa_mode: false,
            auto_select_quantization: false,
        }
    }

    #[tokio::test]
    async fn health_is_public_even_with_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(
            test_runtime_config(dir.path()),
            Box::new(inference_engine::mock_backend::MockBackend::new()),
        )
        .await
        .unwrap();
        let app = create_router(AppState::new(runtime, Some("secret".into())));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_requires_bearer_token_when_configured() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(
            test_runtime_config(dir.path()),
            Box::new(inference_engine::mock_backend::MockBackend::new()),
        )
        .await
        .unwrap();
        let app = create_router(AppState::new(runtime, Some("secret".into())));

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_models_returns_registered_entries() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(
            test_runtime_config(dir.path()),
            Box::new(inference_engine::mock_backend::MockBackend::new()),
        )
        .await
        .unwrap();

        let model_path = dir.path().join("test.gguf");
        tokio::fs::write(&model_path, b"dummy").await.unwrap();
        let sha = model_manager::compute_sha256(&model_path).await.unwrap();

        runtime
            .registry()
            .register(ModelMetadata {
                id: ModelId::from("tiny"),
                name: "Tiny".into(),
                path: model_path,
                quantization: QuantType::Q4KM,
                size_bytes: 5,
                estimated_ram_bytes: 10,
                context_limit: 2048,
                sha256: sha,
                last_used: chrono::Utc::now(),
                download_url: None,
                license: "unknown".into(),
                min_ram_bytes: 0,
                tags: vec![],
            })
            .await
            .unwrap();

        let app = create_router(AppState::new(runtime, None));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["id"], "tiny");
    }

    #[tokio::test]
    async fn download_job_progress_endpoint_reaches_success() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(
            test_runtime_config(dir.path()),
            Box::new(inference_engine::mock_backend::MockBackend::new()),
        )
        .await
        .unwrap();
        let app = create_router(AppState::new(runtime, None));

        let source = dir.path().join("source.gguf");
        let destination = dir.path().join("dest.gguf");
        tokio::fs::write(&source, vec![3_u8; 2048]).await.unwrap();

        let payload = serde_json::json!({
            "id": "phi3-q4",
            "name": "Phi 3 Mini Q4",
            "quant": "Q4KM",
            "source_path": source,
            "source_url": null,
            "destination_path": destination,
        });

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/models/download")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let job: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let job_id = job["job_id"].as_str().unwrap().to_string();

        let mut status = String::new();
        for _ in 0..60 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/v1/models/downloads/{job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
            let snapshot: serde_json::Value = serde_json::from_slice(&body).unwrap();
            status = snapshot["status"].as_str().unwrap().to_string();
            if status == "succeeded" || status == "failed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        assert_eq!(status, "succeeded");
    }
}
