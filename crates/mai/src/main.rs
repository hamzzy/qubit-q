use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::io::AsyncWriteExt;
use tracing_subscriber::EnvFilter;

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

fn create_backend() -> Result<Box<dyn InferenceBackend>> {
    #[cfg(feature = "llama-backend")]
    {
        let backend = inference_engine::llama_backend::LlamaBackendWrapper::new()?;
        Ok(Box::new(backend))
    }

    #[cfg(all(feature = "mock-backend", not(feature = "llama-backend")))]
    {
        Ok(Box::new(inference_engine::mock_backend::MockBackend::new()))
    }

    #[cfg(not(any(feature = "llama-backend", feature = "mock-backend")))]
    {
        anyhow::bail!("No inference backend enabled. Build with --features llama-backend or --features mock-backend")
    }
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
