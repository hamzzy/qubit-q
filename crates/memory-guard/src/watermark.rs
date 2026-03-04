use std::sync::Mutex;

use model_manager::ModelMetadata;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::detector::SystemMemoryDetector;
use crate::error::MemoryError;
use crate::guard::MemoryGuard;
use crate::monitor::{MemoryEvent, MemoryMonitor};

pub const CRITICAL_WATERMARK_PCT: f32 = 0.90;
pub const WARNING_WATERMARK_PCT: f32 = 0.75;
pub const AFRICA_SAFETY_MARGIN: f32 = 0.30;
pub const DEFAULT_SAFETY_MARGIN: f32 = 0.25;

/// Memory guard implementation using watermark thresholds.
pub struct WatermarkGuard {
    detector: Mutex<SystemMemoryDetector>,
    africa_mode: bool,
    safety_margin: f32,
    monitor: Mutex<Option<MemoryMonitor>>,
}

impl WatermarkGuard {
    pub fn new(africa_mode: bool, safety_margin: Option<f32>) -> Self {
        let margin = safety_margin.unwrap_or(if africa_mode {
            AFRICA_SAFETY_MARGIN
        } else {
            DEFAULT_SAFETY_MARGIN
        });

        Self {
            detector: Mutex::new(SystemMemoryDetector::new()),
            africa_mode,
            safety_margin: margin,
            monitor: Mutex::new(None),
        }
    }

    /// Whether Africa mode is enabled.
    pub fn is_africa_mode(&self) -> bool {
        self.africa_mode
    }

    /// Current safety margin as a fraction.
    pub fn safety_margin(&self) -> f32 {
        self.safety_margin
    }

    /// Whether RAM is being simulated.
    pub fn is_simulated(&self) -> bool {
        self.detector.lock().unwrap().is_simulated()
    }

    fn suggest_alternative(&self, model: &ModelMetadata) -> Option<String> {
        Some(format!(
            "Try a smaller quantization (current: {}), or free memory by closing other apps",
            model.quantization
        ))
    }
}

impl MemoryGuard for WatermarkGuard {
    fn can_load_model(&self, model: &ModelMetadata) -> Result<(), MemoryError> {
        let free = self.free_memory_bytes();
        let total = self.total_memory_bytes();
        let required = model.estimated_ram_bytes;
        let used_pct = if total > 0 {
            (total - free) as f32 / total as f32
        } else {
            0.0
        };

        if used_pct > CRITICAL_WATERMARK_PCT {
            warn!(
                used_pct = format!("{:.1}%", used_pct * 100.0),
                "System memory critically high, refusing model load"
            );
            return Err(MemoryError::InsufficientMemory {
                required,
                available: 0,
                suggestion: self.suggest_alternative(model),
            });
        }

        if used_pct > WARNING_WATERMARK_PCT {
            warn!(
                used_pct = format!("{:.1}%", used_pct * 100.0),
                "System memory usage above warning threshold"
            );
        }

        let safe_available = (free as f32 * (1.0 - self.safety_margin)) as u64;

        if required > safe_available {
            return Err(MemoryError::InsufficientMemory {
                required,
                available: safe_available,
                suggestion: self.suggest_alternative(model),
            });
        }

        info!(
            required_mb = required / (1024 * 1024),
            available_mb = safe_available / (1024 * 1024),
            "Memory check passed"
        );
        Ok(())
    }

    fn free_memory_bytes(&self) -> u64 {
        self.detector.lock().unwrap().available_ram()
    }

    fn total_memory_bytes(&self) -> u64 {
        self.detector.lock().unwrap().total_ram()
    }

    fn request_eviction(&self) -> Result<(), MemoryError> {
        warn!("Eviction requested — runtime should handle this");
        Err(MemoryError::EvictionFailed)
    }

    fn start_monitor(&self, interval_ms: u64) -> Option<mpsc::Receiver<MemoryEvent>> {
        let (monitor, rx) =
            MemoryMonitor::start(interval_ms, WARNING_WATERMARK_PCT, CRITICAL_WATERMARK_PCT);
        *self.monitor.lock().unwrap() = Some(monitor);
        Some(rx)
    }

    fn stop_monitor(&self) {
        if let Some(monitor) = self.monitor.lock().unwrap().take() {
            monitor.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model_manager::QuantType;
    use std::path::PathBuf;

    fn test_metadata(estimated_ram: u64) -> ModelMetadata {
        ModelMetadata {
            id: "test".into(),
            name: "Test".into(),
            path: PathBuf::from("/tmp/test.gguf"),
            quantization: QuantType::Q4KM,
            size_bytes: 1_000_000,
            estimated_ram_bytes: estimated_ram,
            context_limit: 2048,
            sha256: "abc".into(),
            last_used: chrono::Utc::now(),
            download_url: None,
            license: "MIT".into(),
            min_ram_bytes: 0,
            tags: vec![],
        }
    }

    #[test]
    fn test_can_load_small_model() {
        let guard = WatermarkGuard::new(false, None);
        let free = guard.free_memory_bytes();
        let total = guard.total_memory_bytes();
        let used_pct = (total - free) as f64 / total as f64;

        if used_pct > 0.90 {
            eprintln!(
                "Skipping test: system memory critically high ({:.1}%)",
                used_pct * 100.0
            );
            return;
        }

        let model = test_metadata(1024);
        assert!(guard.can_load_model(&model).is_ok());
    }

    #[test]
    fn test_rejects_huge_model() {
        let guard = WatermarkGuard::new(false, None);
        let total = guard.total_memory_bytes();
        let model = test_metadata(total * 2);
        assert!(guard.can_load_model(&model).is_err());
    }

    #[test]
    fn test_africa_mode_stricter() {
        let normal = WatermarkGuard::new(false, None);
        let africa = WatermarkGuard::new(true, None);
        assert!(africa.safety_margin() > normal.safety_margin());
    }
}
