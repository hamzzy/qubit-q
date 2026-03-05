use std::convert::Infallible;
use std::time::Duration;

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

/// How long to wait for the inference semaphore before returning 503.
const INFERENCE_QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: Option<bool>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub repeat_penalty: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
    pub seed: Option<u64>,
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

    // Acquire inference slot with timeout — returns 503 if another request is
    // already running and doesn't finish within INFERENCE_QUEUE_TIMEOUT.
    let semaphore = state.inference_semaphore.clone();
    let _permit = tokio::time::timeout(
        INFERENCE_QUEUE_TIMEOUT,
        semaphore.acquire_owned(),
    )
    .await
    .map_err(|_| {
        HttpServerError::Busy("inference engine is busy, try again later".into())
    })?
    .map_err(|_| HttpServerError::Internal("semaphore closed".into()))?;

    state.metrics.inc_inference_total();

    state.runtime.load_model(&req.model).await.map_err(|e| {
        state.metrics.inc_inference_errors();
        HttpServerError::Runtime(e.to_string())
    })?;

    let prompt = build_prompt_from_messages(&req.messages);

    let params = GenerationParams {
        max_tokens: req.max_tokens.unwrap_or(256),
        temperature: req.temperature.unwrap_or(0.7),
        top_p: req.top_p.unwrap_or(0.9),
        top_k: req.top_k.unwrap_or(40),
        repeat_penalty: req.repeat_penalty.unwrap_or(1.1),
        stop_sequences: req.stop_sequences.unwrap_or_default(),
        seed: req.seed,
        ..Default::default()
    };

    let inference_timeout = Duration::from_secs(state.runtime.config().inference_timeout_secs);

    let (mut rx, cancel) = state
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

        // The semaphore permit is moved into the streaming task so it is held
        // for the entire duration of generation, preventing a second request
        // from loading a different model mid-stream.
        tokio::spawn(async move {
            let _permit = _permit;

            let timeout_result = tokio::time::timeout(inference_timeout, async {
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
            })
            .await;

            if timeout_result.is_err() {
                cancel.cancel();
                let _ = tx
                    .send(
                        serde_json::json!({
                            "error": "inference timed out"
                        })
                        .to_string(),
                    )
                    .await;
            }

            let _ = tx.send("[DONE]".to_string()).await;
            metrics.dec_active_streams();
        });

        let stream = ReceiverStream::new(stream_rx)
            .map(|data| Ok::<Event, Infallible>(Event::default().data(data)));

        return Ok(Sse::new(stream).into_response());
    }

    // Non-streaming: collect all tokens with timeout.
    let mut output = String::new();
    let collect_result = tokio::time::timeout(inference_timeout, async {
        while let Some(token) = rx.recv().await {
            output.push_str(&token.text);
        }
    })
    .await;

    if collect_result.is_err() {
        cancel.cancel();
        state.metrics.inc_inference_errors();
        return Err(HttpServerError::Runtime(format!(
            "inference timed out after {}s",
            inference_timeout.as_secs()
        )));
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

fn build_prompt_from_messages(messages: &[ChatMessage]) -> String {
    // Encode the full conversation history using tagged sections so the backend
    // can reconstruct proper chat-template messages for every turn.
    let mut out = String::new();
    for msg in messages {
        let role = msg.role.trim().to_ascii_lowercase();
        let content = msg.content.trim();
        if content.is_empty() {
            continue;
        }
        out.push_str(&format!("[{}]\n{}\n\n", role, content));
    }
    out.trim_end().to_string()
}
