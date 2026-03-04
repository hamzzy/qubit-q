use axum::extract::State;
use axum::http::{header, Request};
use axum::middleware::Next;
use axum::response::Response;

use crate::error::HttpServerError;
use crate::state::AppState;

pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, HttpServerError> {
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let Some(expected) = state.api_key.as_ref() else {
        return Ok(next.run(request).await);
    };

    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);

    if token == Some(expected.as_str()) {
        Ok(next.run(request).await)
    } else {
        Err(HttpServerError::Unauthorized)
    }
}
