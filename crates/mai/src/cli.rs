use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mai")]
#[command(about = "Mobile AI Runtime — Local LLM inference engine")]
#[command(version)]
pub struct Cli {
    /// Path to config file (JSON)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Enable Africa mode (aggressive memory savings)
    #[arg(long, global = true)]
    pub africa_mode: bool,

    /// Models directory
    #[arg(long, global = true)]
    pub models_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run inference on a model
    Run {
        /// Model ID (e.g., phi-3-mini-q4)
        #[arg(long)]
        model: String,

        /// Prompt text
        #[arg(long)]
        prompt: String,

        /// Maximum tokens to generate
        #[arg(long, default_value = "512")]
        max_tokens: usize,

        /// Temperature for sampling
        #[arg(long, default_value = "0.7")]
        temperature: f32,

        /// Top-p sampling
        #[arg(long, default_value = "0.9")]
        top_p: f32,

        /// Path to GGUF file (skip registry lookup)
        #[arg(long)]
        model_path: Option<PathBuf>,
    },

    /// Register a model in the local registry
    Register {
        /// Model ID
        #[arg(long)]
        id: String,

        /// Human-readable name
        #[arg(long)]
        name: String,

        /// Path to .gguf file
        #[arg(long)]
        path: PathBuf,

        /// Quantization type (e.g., Q4KM, Q3KS, Q5KM)
        #[arg(long)]
        quant: String,

        /// SHA256 hash of the model file (computed if not provided)
        #[arg(long)]
        sha256: Option<String>,

        /// Context limit
        #[arg(long, default_value = "2048")]
        context_limit: usize,

        /// Estimated RAM bytes when loaded
        #[arg(long, default_value = "0")]
        estimated_ram: u64,
    },

    /// List registered models
    Models,

    /// Verify a model's SHA256 hash
    Verify {
        /// Model ID to verify
        #[arg(long)]
        model: String,
    },

    /// Show system memory information
    Info,
}
