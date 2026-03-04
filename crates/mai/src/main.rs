use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::io::AsyncWriteExt;
use tracing_subscriber::EnvFilter;

use http_server::ServerConfig;
use inference_engine::InferenceBackend;
use memory_guard::MemoryGuard;
use model_manager::{GenerationParams, ModelId, ModelMetadata, ModelRegistry, QuantType};
use runtime_core::{Runtime, RuntimeConfig};

mod cli;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Build config
    let mut config = RuntimeConfig::load(cli.config.as_deref()).context("Failed to load config")?;

    if cli.africa_mode {
        config.africa_mode = true;
    }
    if let Some(dir) = cli.models_dir {
        config.models_dir = dir;
    }
    config.ensure_dirs()?;

    match cli.command {
        Commands::Run {
            model,
            prompt,
            max_tokens,
            temperature,
            top_p,
            model_path,
        } => {
            cmd_run(
                config,
                model,
                prompt,
                max_tokens,
                temperature,
                top_p,
                model_path,
            )
            .await?;
        }
        Commands::Register {
            id,
            name,
            path,
            quant,
            sha256,
            context_limit,
            estimated_ram,
        } => {
            cmd_register(
                config,
                id,
                name,
                path,
                quant,
                sha256,
                context_limit,
                estimated_ram,
            )
            .await?;
        }
        Commands::Models => {
            cmd_models(config).await?;
        }
        Commands::Verify { model } => {
            cmd_verify(config, model).await?;
        }
        Commands::Info => {
            cmd_info(config)?;
        }
        Commands::Profile { benchmark_model } => {
            cmd_profile(config, benchmark_model).await?;
        }
        Commands::Serve {
            port,
            lan,
            api_key,
            tls_cert,
            tls_key,
        } => {
            cmd_serve(config, port, lan, api_key, tls_cert, tls_key).await?;
        }
    }

    Ok(())
}

async fn cmd_run(
    config: RuntimeConfig,
    model: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    top_p: f32,
    model_path: Option<PathBuf>,
) -> Result<()> {
    let backend = create_backend()?;
    let runtime = Runtime::new(config, backend).await?;

    // Load model
    if let Some(path) = &model_path {
        eprintln!("Loading model from {}...", path.display());
        runtime.load_model_from_path(path, &model).await?;
    } else {
        eprintln!("Loading model '{model}'...");
        runtime.load_model(&model).await?;
    }

    eprintln!("Model loaded. Generating...\n");

    let params = GenerationParams {
        max_tokens,
        temperature,
        top_p,
        ..Default::default()
    };

    let (mut rx, cancel) = runtime.run_inference(&prompt, params).await?;

    // Handle Ctrl+C
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        eprintln!("\nCancelling...");
        cancel_clone.cancel();
    });

    // Stream tokens to stdout
    let mut stdout = tokio::io::stdout();
    let mut total_tokens = 0;
    while let Some(token) = rx.recv().await {
        stdout.write_all(token.text.as_bytes()).await?;
        stdout.flush().await?;
        total_tokens += 1;
    }
    println!();

    eprintln!("\n[{total_tokens} tokens generated]");
    runtime.shutdown().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_register(
    config: RuntimeConfig,
    id: String,
    name: String,
    path: PathBuf,
    quant: String,
    sha256: Option<String>,
    context_limit: usize,
    estimated_ram: u64,
) -> Result<()> {
    let path = std::fs::canonicalize(&path)
        .with_context(|| format!("Model file not found: {}", path.display()))?;

    let quantization: QuantType = quant.parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let size_bytes = std::fs::metadata(&path)?.len();

    // Compute SHA256 if not provided
    let sha256 = match sha256 {
        Some(h) => h,
        None => {
            eprintln!("Computing SHA256 (this may take a moment for large files)...");
            model_manager::compute_sha256(&path).await?
        }
    };

    let metadata = ModelMetadata {
        id: ModelId::from(id.as_str()),
        name,
        path,
        quantization,
        size_bytes,
        estimated_ram_bytes: if estimated_ram > 0 {
            estimated_ram
        } else {
            size_bytes * 2
        },
        context_limit,
        sha256: sha256.clone(),
        last_used: chrono::Utc::now(),
        download_url: None,
        license: "unknown".into(),
        min_ram_bytes: 0,
        tags: vec![],
    };

    let registry = model_manager::InMemoryRegistry::new(&config.models_dir)?;
    registry.register(metadata).await?;

    eprintln!("Model '{id}' registered successfully (SHA256: {sha256})");
    Ok(())
}

async fn cmd_models(config: RuntimeConfig) -> Result<()> {
    let registry = model_manager::InMemoryRegistry::new(&config.models_dir)?;
    let models = registry.list_all().await?;

    if models.is_empty() {
        println!("No models registered. Use 'mai register' to add a model.");
        return Ok(());
    }

    println!(
        "{:<20} {:<25} {:<10} {:<12} PATH",
        "ID", "NAME", "QUANT", "SIZE"
    );
    println!("{}", "-".repeat(90));

    for model in &models {
        let size = format_bytes(model.size_bytes);
        println!(
            "{:<20} {:<25} {:<10} {:<12} {}",
            model.id,
            truncate(&model.name, 24),
            model.quantization,
            size,
            model.path.display(),
        );
    }

    println!("\n{} model(s) registered", models.len());
    Ok(())
}

async fn cmd_verify(config: RuntimeConfig, model_id: String) -> Result<()> {
    let registry = model_manager::InMemoryRegistry::new(&config.models_dir)?;
    let id = ModelId::from(model_id.as_str());
    let metadata = registry
        .get(&id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Model '{model_id}' not found in registry"))?;

    eprintln!("Verifying {}...", metadata.path.display());
    match model_manager::verify_sha256(&metadata.path, &metadata.sha256).await {
        Ok(()) => {
            println!("SHA256 verification PASSED for '{model_id}'");
            println!("Hash: {}", metadata.sha256);
        }
        Err(e) => {
            eprintln!("SHA256 verification FAILED: {e}");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_info(config: RuntimeConfig) -> Result<()> {
    let guard = memory_guard::WatermarkGuard::new(
        config.africa_mode,
        Some(config.memory_safety_margin_pct),
    );

    let total = guard.total_memory_bytes();
    let free = guard.free_memory_bytes();
    let used = total - free;
    let used_pct = used as f64 / total as f64 * 100.0;

    println!("System Memory Info");
    println!("==================");
    println!("Total RAM:     {}", format_bytes(total));
    println!("Used RAM:      {} ({:.1}%)", format_bytes(used), used_pct);
    println!("Available RAM: {}", format_bytes(free));
    println!();
    println!("Runtime Config");
    println!("==============");
    println!("Models dir:    {}", config.models_dir.display());
    println!("Africa mode:   {}", config.africa_mode);
    println!(
        "Safety margin: {:.0}%",
        config.memory_safety_margin_pct * 100.0
    );
    println!("Max context:   {} tokens", config.max_context_tokens);

    Ok(())
}

async fn cmd_profile(config: RuntimeConfig, benchmark_model: Option<PathBuf>) -> Result<()> {
    let profile = device_profiler::SystemProfiler::detect()
        .map_err(|e| anyhow::anyhow!("Failed to detect device profile: {e}"))?;
    let recommended_quant = device_profiler::recommend_quantization(&profile);
    let max_model_size =
        device_profiler::max_model_size_bytes(&profile, config.memory_safety_margin_pct);

    println!("Device Profile");
    println!("==============");
    println!("Platform:      {}", profile.platform);
    println!("CPU arch:      {}", profile.cpu_arch);
    println!("CPU cores:     {}", profile.cpu_cores);
    println!("GPU:           {}", profile.gpu_type);
    println!("Total RAM:     {}", format_bytes(profile.total_ram_bytes));
    println!("Free RAM:      {}", format_bytes(profile.free_ram_bytes));
    println!(
        "Storage free:  {}",
        format_bytes(profile.available_storage_bytes)
    );
    println!();
    println!("Recommendation");
    println!("==============");
    println!("Best quant:    {}", recommended_quant);
    println!("Max model RAM: {}", format_bytes(max_model_size));
    println!(
        "Safety margin: {:.0}%",
        config.memory_safety_margin_pct * 100.0
    );

    if let Some(path) = benchmark_model {
        println!();
        println!("Benchmark");
        println!("=========");
        let result = device_profiler::benchmark_model_tokens_per_sec(&path)
            .await
            .map_err(|e| anyhow::anyhow!("Benchmark failed: {e}"))?;
        println!("Model path:    {}", path.display());
        println!("Tokens/sec:    {:.2}", result.tokens_per_sec);
        println!("Cache hit:     {}", result.cache_hit);
        println!("Measured at:   {}", result.measured_at.to_rfc3339());
    }

    Ok(())
}

async fn cmd_serve(
    runtime_config: RuntimeConfig,
    port: u16,
    lan: bool,
    api_key: Option<String>,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
) -> Result<()> {
    if tls_cert.is_some() ^ tls_key.is_some() {
        anyhow::bail!("TLS requires both --tls-cert and --tls-key");
    }

    let server_config = ServerConfig {
        port,
        lan_mode: lan,
        api_key,
        tls_cert_path: tls_cert,
        tls_key_path: tls_key,
        ..Default::default()
    };

    eprintln!(
        "Starting HTTP server on {}:{} (lan_mode={}, tls={})",
        if lan { "0.0.0.0" } else { "127.0.0.1" },
        port,
        lan,
        server_config.tls_enabled()
    );

    http_server::run_server(server_config, runtime_config)
        .await
        .map_err(|e| anyhow::anyhow!("HTTP server failed: {e}"))
}

fn create_backend() -> Result<Box<dyn InferenceBackend>> {
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
        other => {
            anyhow::bail!("Unknown MAI_BACKEND='{other}'. Supported values: auto, mlx, llama, mock")
        }
    }
}

fn create_backend_auto() -> Result<Box<dyn InferenceBackend>> {
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

    anyhow::bail!(
        "No inference backend enabled. Build with --features mock-backend|llama-backend|mlx-backend"
    )
}

#[cfg(feature = "mlx-backend")]
fn create_backend_mlx() -> Result<Box<dyn InferenceBackend>> {
    Ok(Box::new(inference_engine::mlx_backend::MlxBackend::new()))
}

#[cfg(not(feature = "mlx-backend"))]
fn create_backend_mlx() -> Result<Box<dyn InferenceBackend>> {
    anyhow::bail!("MLX backend requested but this binary was built without 'mlx-backend'")
}

#[cfg(feature = "llama-backend")]
fn create_backend_llama() -> Result<Box<dyn InferenceBackend>> {
    let backend = inference_engine::llama_backend::LlamaBackendWrapper::new()?;
    Ok(Box::new(backend))
}

#[cfg(not(feature = "llama-backend"))]
fn create_backend_llama() -> Result<Box<dyn InferenceBackend>> {
    anyhow::bail!("llama backend requested but this binary was built without 'llama-backend'")
}

#[cfg(feature = "mock-backend")]
fn create_backend_mock() -> Result<Box<dyn InferenceBackend>> {
    Ok(Box::new(inference_engine::mock_backend::MockBackend::new()))
}

#[cfg(not(feature = "mock-backend"))]
fn create_backend_mock() -> Result<Box<dyn InferenceBackend>> {
    anyhow::bail!("mock backend requested but this binary was built without 'mock-backend'")
}

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}
