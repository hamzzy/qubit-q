use std::path::Path;

use async_trait::async_trait;

use crate::error::ProfilerError;
use crate::profile::DeviceProfile;
use model_manager::QuantType;

/// Trait for device profiling and benchmarking.
#[async_trait]
pub trait DeviceProfilerTrait: Send + Sync {
    async fn profile(&self) -> Result<DeviceProfile, ProfilerError>;
    async fn benchmark_tokens_per_sec(&self, model_path: &Path)
        -> Result<f32, ProfilerError>;
    fn recommend_quantization(&self, profile: &DeviceProfile) -> QuantType;
}
