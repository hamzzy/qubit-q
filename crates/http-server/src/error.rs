use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("service busy: {0}")]
    Busy(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for HttpServerError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::Runtime(m) => (StatusCode::UNPROCESSABLE_ENTITY, m),
            Self::Busy(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };

        (status, Json(json!({"error": msg}))).into_response()
    }
}
