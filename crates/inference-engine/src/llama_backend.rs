use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::LlamaTokenAttr;
use llama_cpp_2::TokenToStringError;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use model_manager::GenerationParams;

use crate::backend::{CompletionStats, InferenceBackend, ModelConfig, Token};
use crate::error::InferenceError;

const MAX_CONSECUTIVE_SPECIAL_TOKENS: usize = 24;
const DEFAULT_REPEAT_LAST_N: i32 = 64;

// ── Dedicated inference thread ──────────────────────────────────────────────
//
// All llama.cpp types (LlamaBackend, LlamaModel) live exclusively on a single
// OS thread. No `unsafe impl Send`, no `SendWrapper`. The async world
// communicates with this thread via a command channel.

enum Command {
    LoadModel {
        path: PathBuf,
        config: ModelConfig,
        reply: oneshot::Sender<Result<u64, InferenceError>>,
    },
    UnloadModel {
        reply: oneshot::Sender<Result<(), InferenceError>>,
    },
    StreamCompletion {
        prompt: String,
        params: GenerationParams,
        token_tx: mpsc::Sender<Token>,
        cancel: CancellationToken,
        reply: oneshot::Sender<Result<CompletionStats, InferenceError>>,
    },
    Shutdown,
}

/// Run the inference thread. Owns all non-Send llama.cpp state.
fn inference_thread_main(rx: std::sync::mpsc::Receiver<Command>) {
    let backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "Failed to init llama backend on inference thread");
            // Drain and reply with errors
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Command::LoadModel { reply, .. } => {
                        let _ = reply.send(Err(InferenceError::ModelLoadFailed(format!(
                            "Backend init failed: {e}"
                        ))));
                    }
                    Command::UnloadModel { reply } => {
                        let _ = reply.send(Ok(()));
                    }
                    Command::StreamCompletion { reply, .. } => {
                        let _ = reply.send(Err(InferenceError::InferenceFailed(format!(
                            "Backend init failed: {e}"
                        ))));
                    }
                    Command::Shutdown => break,
                }
            }
            return;
        }
    };

    // Model is heap-allocated for a stable address so LlamaContext can borrow it.
    let mut model: Option<Box<LlamaModel>> = None;
    // SAFETY: ctx borrows from model (Box, stable address) and backend (stack local,
    // never moves). We always drop ctx before model in every code path. The 'static
    // lifetime is a lie that the borrow checker can't track here; we enforce correct
    // drop order manually.
    let mut ctx: Option<LlamaContext<'static>> = None;
    let mut cached_tokens: Vec<LlamaToken> = Vec::new();
    let mut context_size: usize = 2048;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::LoadModel {
                path,
                config,
                reply,
            } => {
                info!(path = %path.display(), ctx = config.context_size, "Loading model with llama.cpp");
                // Drop context before model — it borrows from model.
                ctx = None;
                cached_tokens.clear();
                context_size = config.context_size;

                let result = (|| {
                    let mut model_params = LlamaModelParams::default();
                    if let Some(layers) = config.gpu_layers {
                        model_params = model_params.with_n_gpu_layers(layers);
                        info!(gpu_layers = layers, "GPU layers configured");
                    }
                    let m = LlamaModel::load_from_file(&backend, &path, &model_params)
                        .map_err(|e| InferenceError::ModelLoadFailed(format!("{e}")))?;
                    let size = std::fs::metadata(&config.path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    Ok((m, size))
                })();

                match result {
                    Ok((m, size)) => {
                        model = Some(Box::new(m));
                        info!("Model loaded successfully");
                        let _ = reply.send(Ok(size));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }

            Command::UnloadModel { reply } => {
                ctx = None; // drop context before model
                cached_tokens.clear();
                model = None;
                info!("Model unloaded");
                let _ = reply.send(Ok(()));
            }

            Command::StreamCompletion {
                prompt,
                params,
                token_tx,
                cancel,
                reply,
            } => {
                let Some(ref m) = model else {
                    let _ = reply.send(Err(InferenceError::NoModelLoaded));
                    continue;
                };

                let result = run_inference_loop(
                    &backend,
                    m,
                    &prompt,
                    &params,
                    context_size,
                    &mut ctx,
                    &mut cached_tokens,
                    token_tx,
                    cancel,
                );
                let _ = reply.send(result);
            }

            Command::Shutdown => {
                drop(ctx); // drop context before model — it borrows from model
                drop(model);
                info!("Inference thread shutting down");
                break;
            }
        }
    }
}

// ── Public wrapper ──────────────────────────────────────────────────────────

/// llama.cpp inference backend using the llama-cpp-2 crate.
///
/// All llama.cpp state lives on a dedicated OS thread. This struct holds only
/// a channel sender and shared atomics — it is trivially Send + Sync without
/// any `unsafe impl`.
pub struct LlamaBackendWrapper {
    cmd_tx: std::sync::mpsc::Sender<Command>,
    loaded: Arc<AtomicBool>,
    model_size_bytes: Arc<AtomicU64>,
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl LlamaBackendWrapper {
    pub fn new() -> Result<Self, InferenceError> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();

        let thread = std::thread::Builder::new()
            .name("llama-inference".into())
            .spawn(move || inference_thread_main(cmd_rx))
            .map_err(|e| {
                InferenceError::ModelLoadFailed(format!("Failed to spawn inference thread: {e}"))
            })?;

        Ok(Self {
            cmd_tx,
            loaded: Arc::new(AtomicBool::new(false)),
            model_size_bytes: Arc::new(AtomicU64::new(0)),
            _thread: Some(thread),
        })
    }
}

impl Drop for LlamaBackendWrapper {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Command::Shutdown);
        if let Some(handle) = self._thread.take() {
            let _ = handle.join();
        }
    }
}

#[async_trait]
impl InferenceBackend for LlamaBackendWrapper {
    async fn load_model(
        &mut self,
        path: &Path,
        config: &ModelConfig,
    ) -> Result<(), InferenceError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::LoadModel {
                path: path.to_path_buf(),
                config: config.clone(),
                reply: reply_tx,
            })
            .map_err(|_| InferenceError::ModelLoadFailed("inference thread gone".into()))?;

        let size = reply_rx
            .await
            .map_err(|_| InferenceError::ModelLoadFailed("inference thread dropped reply".into()))??;

        self.model_size_bytes.store(size, Ordering::Relaxed);
        self.loaded.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn unload_model(&mut self) -> Result<(), InferenceError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::UnloadModel { reply: reply_tx })
            .map_err(|_| InferenceError::InferenceFailed("inference thread gone".into()))?;

        reply_rx
            .await
            .map_err(|_| InferenceError::InferenceFailed("inference thread dropped reply".into()))??;

        self.model_size_bytes.store(0, Ordering::Relaxed);
        self.loaded.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn stream_completion(
        &self,
        prompt: &str,
        params: &GenerationParams,
        tx: mpsc::Sender<Token>,
        cancel: CancellationToken,
    ) -> Result<CompletionStats, InferenceError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StreamCompletion {
                prompt: prompt.to_string(),
                params: params.clone(),
                token_tx: tx,
                cancel,
                reply: reply_tx,
            })
            .map_err(|_| InferenceError::InferenceFailed("inference thread gone".into()))?;

        reply_rx
            .await
            .map_err(|_| InferenceError::InferenceFailed("inference thread dropped reply".into()))?
    }

    fn memory_usage_bytes(&self) -> u64 {
        self.model_size_bytes.load(Ordering::Relaxed)
    }

    fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }
}

// ── Inference loop (runs on the dedicated thread) ───────────────────────────

fn run_inference_loop(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &str,
    params: &GenerationParams,
    ctx_size: usize,
    persistent_ctx: &mut Option<LlamaContext<'static>>,
    cached_tokens: &mut Vec<LlamaToken>,
    tx: mpsc::Sender<Token>,
    cancel: CancellationToken,
) -> Result<CompletionStats, InferenceError> {
    let (model_prompt, add_bos) = prepare_model_prompt(model, prompt);
    let tokens = model
        .str_to_token(&model_prompt, add_bos)
        .map_err(|e| InferenceError::InferenceFailed(format!("Tokenization failed: {e}")))?;

    let prompt_token_count = tokens.len();

    if prompt_token_count >= ctx_size {
        return Err(InferenceError::InferenceFailed(format!(
            "Prompt ({prompt_token_count} tokens) exceeds context size ({ctx_size})"
        )));
    }

    // Find how many tokens at the start match the cached KV state.
    let common_prefix = tokens
        .iter()
        .zip(cached_tokens.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let (ctx, decode_from) = if common_prefix > 0 && persistent_ctx.is_some() {
        // Reuse existing context — just trim KV cache after the common prefix.
        let ctx = persistent_ctx.as_mut().unwrap();
        let _ = ctx.clear_kv_cache_seq(
            Some(0),
            Some(common_prefix as u32),
            None,
        );
        debug!(
            common_prefix,
            new_tokens = prompt_token_count - common_prefix,
            "Reusing KV cache"
        );
        (ctx, common_prefix)
    } else {
        // Create a fresh context (first request or prompt diverged completely).
        *persistent_ctx = None;
        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZero::new(ctx_size as u32));
        // SAFETY: model is heap-allocated (Box) and backend is a stack local that
        // lives for the entire inference thread. We always drop persistent_ctx
        // before model in every code path (LoadModel, UnloadModel, Shutdown).
        let model_ref: &'static LlamaModel = unsafe { &*(model as *const LlamaModel) };
        let backend_ref: &'static LlamaBackend = unsafe { &*(backend as *const LlamaBackend) };
        let new_ctx = model_ref
            .new_context(backend_ref, ctx_params)
            .map_err(|e| InferenceError::InferenceFailed(format!("Context creation failed: {e}")))?;
        *persistent_ctx = Some(new_ctx);
        debug!(prompt_tokens = prompt_token_count, "Fresh context created");
        (persistent_ctx.as_mut().unwrap(), 0)
    };

    // Decode only the new (non-cached) portion of the prompt.
    let tokens_to_decode = &tokens[decode_from..];
    if !tokens_to_decode.is_empty() {
        let mut batch = LlamaBatch::new(ctx_size, 1);
        for (i, &token) in tokens_to_decode.iter().enumerate() {
            let pos = (decode_from + i) as i32;
            let is_last = i == tokens_to_decode.len() - 1;
            batch
                .add(token, pos, &[0], is_last)
                .map_err(|e| InferenceError::InferenceFailed(format!("Batch add failed: {e}")))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| InferenceError::InferenceFailed(format!("Prompt decode failed: {e}")))?;
    }

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(DEFAULT_REPEAT_LAST_N, params.repeat_penalty, 0.0, 0.0),
        LlamaSampler::top_k(params.top_k as i32),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(params.seed.unwrap_or(0) as u32),
    ]);

    // Batch used for single-token generation steps.
    let mut batch = LlamaBatch::new(ctx_size, 1);
    // Seed batch with the last prompt position so sampler has something to sample from.
    // If we decoded new tokens above, the decode already set this up. If prompt was fully
    // cached (tokens_to_decode was empty), we need a dummy decode at the last position.
    if tokens_to_decode.is_empty() && !tokens.is_empty() {
        let last_pos = (tokens.len() - 1) as i32;
        let last_tok = tokens[tokens.len() - 1];
        batch
            .add(last_tok, last_pos, &[0], true)
            .map_err(|e| InferenceError::InferenceFailed(format!("Batch add failed: {e}")))?;
        ctx.decode(&mut batch)
            .map_err(|e| InferenceError::InferenceFailed(format!("Seed decode failed: {e}")))?;
    }

    let start = Instant::now();
    let mut n_cur = tokens.len();
    let mut generated_tokens: Vec<LlamaToken> = Vec::new();
    let mut generated = 0;
    let mut consecutive_special = 0usize;
    let max_tokens = params.max_tokens.min(ctx_size - prompt_token_count);
    let stop_sequences = normalized_stop_sequences(&params.stop_sequences);
    let max_stop_len = stop_sequences
        .iter()
        .map(std::string::String::len)
        .max()
        .unwrap_or(0);
    let holdback_bytes = max_stop_len.saturating_sub(1);
    let mut pending_text = String::new();
    let mut stop_triggered = false;
    let mut last_token_id: Option<u32> = None;

    while generated < max_tokens {
        if cancel.is_cancelled() {
            info!(generated, "Inference cancelled");
            // Update cached tokens with what we generated so far
            *cached_tokens = tokens.clone();
            cached_tokens.extend_from_slice(&generated_tokens);
            return Err(InferenceError::Cancelled);
        }

        let token_id = sampler.sample(ctx, batch.n_tokens() - 1);

        if model.is_eog_token(token_id) {
            debug!("End of generation token received");
            break;
        }

        generated_tokens.push(token_id);

        let token_is_control = is_control_or_special_token(model, token_id);
        // Render with special=true so we see literal <s>, </s>, etc. for filtering.
        // With special=false some models produce empty bytes for BOS/EOS which then
        // bypass our text-based suppression checks.
        let piece_str = match model.token_to_piece_bytes(token_id, 128, true, None) {
            Ok(piece_bytes) => String::from_utf8_lossy(&piece_bytes).to_string(),
            Err(TokenToStringError::UnknownTokenType) => String::new(),
            Err(e) => {
                return Err(InferenceError::InferenceFailed(format!(
                    "Token decode failed: {e}"
                )))
            }
        };

        let is_special_piece = should_suppress_output_piece(token_is_control, &piece_str);
        if is_special_piece {
            consecutive_special += 1;
        } else {
            consecutive_special = 0;
        }

        if consecutive_special >= MAX_CONSECUTIVE_SPECIAL_TOKENS {
            return Err(InferenceError::InferenceFailed(
                "model emitted only special/control tokens; check model compatibility and prompt template"
                    .to_string(),
            ));
        }

        if !piece_str.is_empty() && !is_special_piece {
            last_token_id = Some(token_id.0 as u32);
            pending_text.push_str(&piece_str);

            if let Some(stop_at) = find_first_stop_index(&pending_text, &stop_sequences) {
                let before_stop = pending_text[..stop_at].to_string();
                if !before_stop.is_empty()
                    && tx
                        .blocking_send(Token {
                            text: before_stop,
                            id: token_id.0 as u32,
                            logprob: None,
                        })
                        .is_err()
                {
                    debug!("Receiver dropped, stopping generation");
                    break;
                }
                stop_triggered = true;
                pending_text.clear();
            } else if holdback_bytes == 0 {
                if tx
                    .blocking_send(Token {
                        text: std::mem::take(&mut pending_text),
                        id: token_id.0 as u32,
                        logprob: None,
                    })
                    .is_err()
                {
                    debug!("Receiver dropped, stopping generation");
                    break;
                }
            } else if pending_text.len() > holdback_bytes {
                let mut flush_len = pending_text.len() - holdback_bytes;
                while flush_len > 0 && !pending_text.is_char_boundary(flush_len) {
                    flush_len -= 1;
                }

                if flush_len > 0 {
                    let emit = pending_text[..flush_len].to_string();
                    pending_text.drain(..flush_len);
                    if tx
                        .blocking_send(Token {
                            text: emit,
                            id: token_id.0 as u32,
                            logprob: None,
                        })
                        .is_err()
                    {
                        debug!("Receiver dropped, stopping generation");
                        break;
                    }
                }
            }
        }

        if stop_triggered {
            debug!("Stop sequence triggered");
            break;
        }

        batch.clear();
        batch
            .add(token_id, n_cur as i32, &[0], true)
            .map_err(|e| InferenceError::InferenceFailed(format!("Batch add failed: {e}")))?;

        ctx.decode(&mut batch)
            .map_err(|e| InferenceError::InferenceFailed(format!("Decode failed: {e}")))?;

        n_cur += 1;
        generated += 1;
    }

    if !pending_text.is_empty() {
        let _ = tx.blocking_send(Token {
            text: pending_text,
            id: last_token_id.unwrap_or(0),
            logprob: None,
        });
    }

    // Update cached tokens: prompt tokens + all generated tokens.
    // Next turn can reuse the KV cache for the common prefix.
    *cached_tokens = tokens;
    cached_tokens.extend_from_slice(&generated_tokens);

    let elapsed = start.elapsed();
    let stats = CompletionStats {
        tokens_generated: generated,
        tokens_per_second: if elapsed.as_secs_f32() > 0.0 {
            generated as f32 / elapsed.as_secs_f32()
        } else {
            0.0
        },
        prompt_tokens: prompt_token_count,
        total_duration_ms: elapsed.as_millis() as u64,
    };

    info!(
        tokens = stats.tokens_generated,
        tps = format!("{:.1}", stats.tokens_per_second),
        duration_ms = stats.total_duration_ms,
        "Inference complete"
    );

    Ok(stats)
}

// ── Prompt preparation (pure functions, no llama.cpp types escape) ──────────

fn prepare_model_prompt(model: &LlamaModel, prompt: &str) -> (String, AddBos) {
    let messages = build_messages(prompt);
    if messages.is_empty() {
        return (prompt.to_string(), AddBos::Always);
    }

    let template = match model.chat_template(None) {
        Ok(template) => template,
        Err(err) => {
            debug!(error = %err, "Model has no chat template; falling back to family prompt");
            return fallback_prompt_by_family(model, prompt);
        }
    };

    match model.apply_chat_template(&template, &messages, true) {
        Ok(rendered) => (rendered, AddBos::Never),
        Err(err) => {
            debug!(
                error = %err,
                "Chat template application failed; falling back to family prompt"
            );
            fallback_prompt_by_family(model, prompt)
        }
    }
}

fn fallback_prompt_by_family(model: &LlamaModel, prompt: &str) -> (String, AddBos) {
    let (system_prompt, user_prompt) = split_prompt_sections(prompt);
    if user_prompt.is_empty() {
        return (prompt.to_string(), AddBos::Always);
    }

    let architecture = model
        .meta_val_str("general.architecture")
        .ok()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let model_name = model
        .meta_val_str("general.name")
        .ok()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let family_hint = format!("{architecture} {model_name}");

    if family_hint.contains("qwen") {
        debug!(family = "qwen", "Applying fallback prompt template");
        let mut rendered = String::new();
        if let Some(system) = system_prompt.filter(|s| !s.trim().is_empty()) {
            rendered.push_str("<|im_start|>system\n");
            rendered.push_str(system.trim());
            rendered.push_str("<|im_end|>\n");
        }
        rendered.push_str("<|im_start|>user\n");
        rendered.push_str(user_prompt.trim());
        rendered.push_str("<|im_end|>\n<|im_start|>assistant\n");
        return (rendered, AddBos::Never);
    }

    if family_hint.contains("phi") {
        debug!(family = "phi", "Applying fallback prompt template");
        let mut rendered = String::new();
        if let Some(system) = system_prompt.filter(|s| !s.trim().is_empty()) {
            rendered.push_str("<|system|>\n");
            rendered.push_str(system.trim());
            rendered.push('\n');
        }
        rendered.push_str("<|user|>\n");
        rendered.push_str(user_prompt.trim());
        rendered.push_str("\n<|assistant|>\n");
        return (rendered, AddBos::Never);
    }

    if family_hint.contains("gemma") {
        debug!(family = "gemma", "Applying fallback prompt template");
        let mut rendered = String::new();
        rendered.push_str("<start_of_turn>user\n");
        rendered.push_str(user_prompt.trim());
        rendered.push_str("\n<end_of_turn>\n<start_of_turn>model\n");
        return (rendered, AddBos::Always);
    }

    if family_hint.contains("llama")
        || family_hint.contains("mistral")
        || family_hint.contains("tinyllama")
    {
        debug!(
            family = "llama/mistral",
            "Applying fallback prompt template"
        );
        let rendered = if let Some(system) = system_prompt.filter(|s| !s.trim().is_empty()) {
            format!(
                "<s>[INST] <<SYS>>\n{}\n<</SYS>>\n\n{} [/INST]",
                system.trim(),
                user_prompt.trim()
            )
        } else {
            format!("<s>[INST] {} [/INST]", user_prompt.trim())
        };
        return (rendered, AddBos::Never);
    }

    debug!(family = "generic", "Applying fallback prompt template");
    if let Some(system) = system_prompt.filter(|s| !s.trim().is_empty()) {
        (
            format!(
                "System: {}\n\nUser: {}\nAssistant:",
                system.trim(),
                user_prompt.trim()
            ),
            AddBos::Always,
        )
    } else {
        (
            format!("User: {}\nAssistant:", user_prompt.trim()),
            AddBos::Always,
        )
    }
}

/// Parse a multi-turn tagged prompt into `LlamaChatMessage` list.
///
/// The wire format produced by the HTTP layer is:
/// ```text
/// [role]
/// content
///
/// [role]
/// content
/// ```
/// where `role` is `system`, `user`, or `assistant` (case-insensitive).
///
/// Falls back to treating the whole string as a single user message when
/// no section tags are found.
fn build_messages(prompt: &str) -> Vec<LlamaChatMessage> {
    parse_tagged_sections(prompt)
        .into_iter()
        .filter(|(_, content)| !content.is_empty())
        .filter_map(|(role, content)| LlamaChatMessage::new(role, content.replace('\0', "")).ok())
        .collect()
}

fn parse_tagged_sections(prompt: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current_role: Option<String> = None;
    let mut current_content = String::new();

    for line in prompt.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let tag = &trimmed[1..trimmed.len() - 1];
            let role_lower = tag.to_ascii_lowercase();
            if matches!(role_lower.as_str(), "system" | "user" | "assistant") {
                if let Some(role) = current_role.take() {
                    sections.push((
                        role,
                        std::mem::take(&mut current_content).trim().to_string(),
                    ));
                }
                current_role = Some(role_lower);
                current_content.clear();
                continue;
            }
        }

        if current_role.is_some() {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    if let Some(role) = current_role {
        let content = current_content.trim().to_string();
        sections.push((role, content));
    }

    if sections.is_empty() && !prompt.trim().is_empty() {
        sections.push(("user".to_string(), prompt.trim().to_string()));
    }

    sections
}

fn split_prompt_sections(prompt: &str) -> (Option<String>, String) {
    let sections = parse_tagged_sections(prompt);

    let system = sections
        .iter()
        .find(|(r, _)| r == "system")
        .map(|(_, c)| c.clone())
        .filter(|s| !s.is_empty());

    let user = sections
        .iter()
        .rfind(|(r, _)| r == "user")
        .map(|(_, c)| c.clone())
        .unwrap_or_default();

    (system, user)
}

fn is_control_or_special_token(model: &LlamaModel, token: llama_cpp_2::token::LlamaToken) -> bool {
    let attrs = model.token_attr(token);
    attrs.contains(LlamaTokenAttr::Control)
}

fn should_suppress_output_piece(is_control_token: bool, piece: &str) -> bool {
    if is_control_token {
        return true;
    }
    if piece.is_empty() {
        return true;
    }
    // Suppress literal special token strings that leak through when the model
    // emits BOS/EOS/etc. tokens that aren't flagged as Control in metadata.
    let trimmed = piece.trim();
    if trimmed == "<s>" || trimmed == "</s>" || trimmed == "<unk>" || trimmed == "<pad>" {
        return true;
    }
    if trimmed.starts_with("<|") && trimmed.ends_with("|>") {
        return true;
    }
    if trimmed.starts_with("<\u{ff5c}") && trimmed.ends_with("\u{ff5c}>") {
        return true;
    }
    false
}

fn normalized_stop_sequences(stop_sequences: &[String]) -> Vec<String> {
    stop_sequences
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect()
}

fn find_first_stop_index(text: &str, stop_sequences: &[String]) -> Option<usize> {
    stop_sequences.iter().filter_map(|seq| text.find(seq)).min()
}

#[cfg(test)]
mod tests {
    use super::{
        find_first_stop_index, normalized_stop_sequences, parse_tagged_sections,
        should_suppress_output_piece, split_prompt_sections,
    };

    #[test]
    fn suppresses_control_or_empty_pieces() {
        assert!(should_suppress_output_piece(true, "hello"));
        assert!(should_suppress_output_piece(false, ""));
        assert!(!should_suppress_output_piece(false, "hello"));
        // Whitespace-only tokens are real content (spaces, newlines)
        assert!(!should_suppress_output_piece(false, " "));
        assert!(!should_suppress_output_piece(false, "   "));
        assert!(!should_suppress_output_piece(false, "\n"));
        // Literal special token strings should be suppressed
        assert!(should_suppress_output_piece(false, "<s>"));
        assert!(should_suppress_output_piece(false, "</s>"));
        assert!(should_suppress_output_piece(false, "<|im_end|>"));
        assert!(should_suppress_output_piece(false, "<|endoftext|>"));
    }

    #[test]
    fn parses_single_user_message() {
        let prompt = "Hello there";
        let sections = parse_tagged_sections(prompt);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].0, "user");
        assert_eq!(sections[0].1, "Hello there");
    }

    #[test]
    fn parses_system_and_user_sections() {
        let prompt = "[system]\nBe concise.\n\n[user]\nHello there";
        let (system, user) = split_prompt_sections(prompt);
        assert_eq!(system.as_deref(), Some("Be concise."));
        assert_eq!(user, "Hello there");
    }

    #[test]
    fn parses_multi_turn_conversation() {
        let prompt =
            "[system]\nBe helpful.\n\n[user]\nHi\n\n[assistant]\nHello!\n\n[user]\nHow are you?";
        let sections = parse_tagged_sections(prompt);
        assert_eq!(sections.len(), 4);
        assert_eq!(
            sections[0],
            ("system".to_string(), "Be helpful.".to_string())
        );
        assert_eq!(sections[1], ("user".to_string(), "Hi".to_string()));
        assert_eq!(sections[2], ("assistant".to_string(), "Hello!".to_string()));
        assert_eq!(
            sections[3],
            ("user".to_string(), "How are you?".to_string())
        );
    }

    #[test]
    fn split_finds_last_user_in_multi_turn() {
        let prompt = "[user]\nFirst message\n\n[assistant]\nResponse\n\n[user]\nFollow-up";
        let (system, user) = split_prompt_sections(prompt);
        assert_eq!(system, None);
        assert_eq!(user, "Follow-up");
    }

    #[test]
    fn normalizes_stop_sequences() {
        let stops = normalized_stop_sequences(&[
            "".to_string(),
            "  ".to_string(),
            "</s>".to_string(),
            " <|im_end|> ".to_string(),
        ]);
        assert_eq!(
            stops,
            vec![
                "  ".to_string(),
                "</s>".to_string(),
                " <|im_end|> ".to_string()
            ]
        );
    }

    #[test]
    fn finds_first_stop_index() {
        let stops = vec!["</s>".to_string(), "<|im_end|>".to_string()];
        assert_eq!(find_first_stop_index("hello</s>world", &stops), Some(5));
        assert_eq!(find_first_stop_index("plain text", &stops), None);
    }
}
