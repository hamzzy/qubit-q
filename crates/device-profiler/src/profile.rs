use serde::{Deserialize, Serialize};

/// Hardware profile of the current device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub total_ram_bytes: u64,
    pub free_ram_bytes: u64,
    pub cpu_cores: u32,
    pub cpu_arch: CpuArch,
    pub has_gpu: bool,
    pub gpu_type: GpuType,
    pub platform: Platform,
    pub battery_level: Option<f32>,
    pub is_charging: bool,
    pub available_storage_bytes: u64,
    pub benchmark_tokens_per_sec: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuType {
    Metal,
    Vulkan,
    #[allow(clippy::upper_case_acronyms)]
    NNAPI,
    None,
}

impl std::fmt::Display for GpuType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuType::Metal => write!(f, "Metal"),
            GpuType::Vulkan => write!(f, "Vulkan"),
            GpuType::NNAPI => write!(f, "NNAPI"),
            GpuType::None => write!(f, "None (CPU only)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    Ios,
    Android,
    MacOs,
    Linux,
    Windows,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Ios => write!(f, "iOS"),
            Platform::Android => write!(f, "Android"),
            Platform::MacOs => write!(f, "macOS"),
            Platform::Linux => write!(f, "Linux"),
            Platform::Windows => write!(f, "Windows"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuArch {
    Aarch64,
    X86_64,
    Armv7,
    Other,
}

impl std::fmt::Display for CpuArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CpuArch::Aarch64 => write!(f, "aarch64"),
            CpuArch::X86_64 => write!(f, "x86_64"),
            CpuArch::Armv7 => write!(f, "armv7"),
            CpuArch::Other => write!(f, "other"),
        }
    }
}

impl DeviceProfile {
    /// RAM available for model loading (in GB).
    pub fn usable_ram_gb(&self) -> f64 {
        self.free_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Total RAM in GB.
    pub fn total_ram_gb(&self) -> f64 {
        self.total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}
