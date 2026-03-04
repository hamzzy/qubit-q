#[derive(Debug, thiserror::Error)]
pub enum ProfilerError {
    #[error("Detection failed: {0}")]
    DetectionFailed(String),

    #[error("Benchmark failed: {0}")]
    BenchmarkFailed(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),
}
