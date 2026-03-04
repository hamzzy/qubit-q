use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;

/// Configuration for the MAI runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub models_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub max_storage_bytes: u64,
    pub max_context_tokens: usize,
    pub memory_safety_margin_pct: f32,
    pub inference_timeout_secs: u64,
    pub africa_mode: bool,
    pub auto_select_quantization: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let mai_dir = home.join(".mai");
        Self {
            models_dir: mai_dir.join("models"),
            cache_dir: mai_dir.join("cache"),
            logs_dir: mai_dir.join("logs"),
            max_storage_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
            max_context_tokens: 2048,
            memory_safety_margin_pct: 0.25,
            inference_timeout_secs: 300,
            africa_mode: false,
            auto_select_quantization: true,
        }
    }
}

impl RuntimeConfig {
    /// Load config from a JSON file, falling back to defaults for missing fields.
    pub fn load(path: Option<&Path>) -> Result<Self, RuntimeError> {
        match path {
            Some(p) => {
                let data = std::fs::read_to_string(p)
                    .map_err(|e| RuntimeError::Config(format!("Failed to read config: {e}")))?;
                serde_json::from_str(&data)
                    .map_err(|e| RuntimeError::Config(format!("Failed to parse config: {e}")))
            }
            None => Ok(Self::default()),
        }
    }

    /// Ensure all required directories exist.
    pub fn ensure_dirs(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.models_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.logs_dir)?;
        Ok(())
    }
}
