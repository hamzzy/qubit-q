use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a model (e.g. "phi-3-mini-q4").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(pub String);

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ModelId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ModelId(s.to_string()))
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        ModelId(s.to_string())
    }
}

/// Quantization type for GGUF models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantType {
    Q2K,
    Q3KS,
    Q3KM,
    Q4KM,
    Q4KS,
    Q5KM,
    Q5KS,
    Q6K,
    Q8_0,
    F16,
}

impl QuantType {
    /// Bits per weight (approximate).
    pub fn bits_per_weight(&self) -> f32 {
        match self {
            QuantType::Q2K => 2.6,
            QuantType::Q3KS => 3.0,
            QuantType::Q3KM => 3.35,
            QuantType::Q4KM => 4.85,
            QuantType::Q4KS => 4.35,
            QuantType::Q5KM => 5.68,
            QuantType::Q5KS => 5.21,
            QuantType::Q6K => 6.56,
            QuantType::Q8_0 => 8.5,
            QuantType::F16 => 16.0,
        }
    }

    /// Estimate RAM usage for a model with `param_billions` billion parameters.
    pub fn estimate_ram_bytes(&self, param_billions: f32) -> u64 {
        let bits = self.bits_per_weight();
        let overhead_factor = 1.20; // KV cache + context overhead
        ((param_billions * 1e9 * bits / 8.0) * overhead_factor) as u64
    }
}

impl fmt::Display for QuantType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            QuantType::Q2K => "Q2_K",
            QuantType::Q3KS => "Q3_K_S",
            QuantType::Q3KM => "Q3_K_M",
            QuantType::Q4KM => "Q4_K_M",
            QuantType::Q4KS => "Q4_K_S",
            QuantType::Q5KM => "Q5_K_M",
            QuantType::Q5KS => "Q5_K_S",
            QuantType::Q6K => "Q6_K",
            QuantType::Q8_0 => "Q8_0",
            QuantType::F16 => "F16",
        };
        write!(f, "{name}")
    }
}

impl FromStr for QuantType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().replace(['_', '-'], "").as_str() {
            "Q2K" => Ok(QuantType::Q2K),
            "Q3KS" => Ok(QuantType::Q3KS),
            "Q3KM" => Ok(QuantType::Q3KM),
            "Q4KM" => Ok(QuantType::Q4KM),
            "Q4KS" => Ok(QuantType::Q4KS),
            "Q5KM" => Ok(QuantType::Q5KM),
            "Q5KS" => Ok(QuantType::Q5KS),
            "Q6K" => Ok(QuantType::Q6K),
            "Q80" => Ok(QuantType::Q8_0),
            "F16" => Ok(QuantType::F16),
            _ => Err(format!("Unknown quantization type: {s}")),
        }
    }
}

/// State of a model in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelState {
    Registered,
    Downloading,
    Verified,
    Loading,
    Active,
    Unloading,
}

/// Metadata describing a registered model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub id: ModelId,
    pub name: String,
    pub path: PathBuf,
    pub quantization: QuantType,
    pub size_bytes: u64,
    pub estimated_ram_bytes: u64,
    pub context_limit: usize,
    pub sha256: String,
    pub last_used: DateTime<Utc>,
    pub download_url: Option<String>,
    pub license: String,
    pub min_ram_bytes: u64,
    pub tags: Vec<String>,
}

/// Parameters for text generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub repeat_penalty: f32,
    pub stop_sequences: Vec<String>,
    pub seed: Option<u64>,
    pub stream: bool,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repeat_penalty: 1.1,
            stop_sequences: vec![],
            seed: None,
            stream: true,
        }
    }
}

#[cfg(test)]
pub fn dummy_metadata() -> ModelMetadata {
    ModelMetadata {
        id: ModelId("test-model".into()),
        name: "Test Model".into(),
        path: PathBuf::from("/tmp/test.gguf"),
        quantization: QuantType::Q4KM,
        size_bytes: 1_000_000,
        estimated_ram_bytes: 2_000_000,
        context_limit: 2048,
        sha256: "abc123".into(),
        last_used: Utc::now(),
        download_url: None,
        license: "MIT".into(),
        min_ram_bytes: 1_500_000,
        tags: vec![],
    }
}
