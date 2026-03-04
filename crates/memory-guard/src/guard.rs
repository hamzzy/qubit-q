use model_manager::ModelMetadata;
use tokio::sync::mpsc;

use crate::error::MemoryError;
use crate::monitor::MemoryEvent;

/// Trait for memory safety checks before loading models.
pub trait MemoryGuard: Send + Sync {
    /// Check if it is safe to load the given model.
    fn can_load_model(&self, model: &ModelMetadata) -> Result<(), MemoryError>;

    /// Current free system memory in bytes.
    fn free_memory_bytes(&self) -> u64;

    /// Total system memory in bytes.
    fn total_memory_bytes(&self) -> u64;

    /// Request model eviction to free memory.
    fn request_eviction(&self) -> Result<(), MemoryError>;

    /// Start background memory monitoring loop.
    /// Returns a receiver for memory events, or None if not supported.
    fn start_monitor(&self, interval_ms: u64) -> Option<mpsc::Receiver<MemoryEvent>>;

    /// Stop background memory monitoring.
    fn stop_monitor(&self);
}
