use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::HttpServerError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EmbeddingsRequest {
    pub model: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingData {
    pub object: &'static str,
    pub index: usize,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingsResponse {
    pub object: &'static str,
    pub data: Vec<EmbeddingData>,
    pub model: String,
}

pub async fn create_embedding(
    State(_state): State<AppState>,
    Json(req): Json<EmbeddingsRequest>,
) -> Result<Json<EmbeddingsResponse>, HttpServerError> {
    let texts = match req.input {
        serde_json::Value::String(s) => vec![s],
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    HttpServerError::BadRequest("input array must contain only strings".into())
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(HttpServerError::BadRequest(
                "input must be a string or array of strings".into(),
            ));
        }
    };

    let data = texts
        .iter()
        .enumerate()
        .map(|(idx, text)| EmbeddingData {
            object: "embedding",
            index: idx,
            embedding: fake_embedding(text, 64),
        })
        .collect();

    Ok(Json(EmbeddingsResponse {
        object: "list",
        data,
        model: req.model,
    }))
}

fn fake_embedding(text: &str, dim: usize) -> Vec<f32> {
    let mut seed = text.as_bytes().iter().fold(0_u64, |acc, b| {
        acc.wrapping_mul(131).wrapping_add(*b as u64)
    });

    (0..dim)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 8) % 10_000) as f32 / 10_000.0
        })
        .collect()
}
