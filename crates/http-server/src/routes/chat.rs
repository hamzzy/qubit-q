use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use model_manager::GenerationParams;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::error::HttpServerError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: Option<bool>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessageResponse,
    pub finish_reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ChatMessageResponse {
    pub role: &'static str,
    pub content: String,
}

pub async fn chat_completion(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Response, HttpServerError> {
    if req.messages.is_empty() {
        return Err(HttpServerError::BadRequest(
            "messages must not be empty".into(),
        ));
    }

    state.metrics.inc_inference_total();

    state.runtime.load_model(&req.model).await.map_err(|e| {
        state.metrics.inc_inference_errors();
        HttpServerError::Runtime(e.to_string())
    })?;

    let prompt = req
        .messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let params = GenerationParams {
        max_tokens: req.max_tokens.unwrap_or(256),
        temperature: req.temperature.unwrap_or(0.7),
        top_p: req.top_p.unwrap_or(0.9),
        ..Default::default()
    };

    let (mut rx, _cancel) = state
        .runtime
        .run_inference(&prompt, params)
        .await
        .map_err(|e| {
            state.metrics.inc_inference_errors();
            HttpServerError::Runtime(e.to_string())
        })?;

    if req.stream.unwrap_or(false) {
        state.metrics.inc_active_streams();
        let metrics = state.metrics.clone();
        let model = req.model.clone();
        let (tx, stream_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            while let Some(token) = rx.recv().await {
                let chunk = serde_json::json!({
                    "id": "chatcmpl-stream",
                    "object": "chat.completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {"content": token.text},
                        "finish_reason": serde_json::Value::Null,
                    }]
                })
                .to_string();

                if tx.send(chunk).await.is_err() {
                    break;
                }
            }

            let _ = tx.send("[DONE]".to_string()).await;
            metrics.dec_active_streams();
        });

        let stream = ReceiverStream::new(stream_rx)
            .map(|data| Ok::<Event, Infallible>(Event::default().data(data)));

        return Ok(Sse::new(stream).into_response());
    }

    let mut output = String::new();
    while let Some(token) = rx.recv().await {
        output.push_str(&token.text);
    }

    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis()),
        object: "chat.completion",
        created: chrono::Utc::now().timestamp(),
        model: req.model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessageResponse {
                role: "assistant",
                content: output,
            },
            finish_reason: "stop",
        }],
    };

    Ok(Json(response).into_response())
}
