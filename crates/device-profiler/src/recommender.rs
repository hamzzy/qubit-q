use model_manager::QuantType;

use crate::profile::DeviceProfile;

/// Recommend the best quantization for the device based on available RAM.
/// Uses the Africa/low-memory device table from the spec:
///
/// | Device RAM | Max Usable | Recommended Quant | Typical Models           |
/// |-----------|-----------|-------------------|--------------------------|
/// | 2 GB      | ~1.2 GB   | Q2_K or Q3_K_S    | Phi-2, TinyLlama 1.1B    |
/// | 3 GB      | ~1.8 GB   | Q4_K_M            | Phi-3 Mini, TinyLlama 3B |
/// | 4 GB      | ~2.5 GB   | Q4_K_M or Q5_K_M  | Mistral 7B Q4            |
/// | 6 GB+     | ~4 GB     | Q5_K_M / Q6_K     | Mistral 7B, Llama 3 8B   |
pub fn recommend_quantization(profile: &DeviceProfile) -> QuantType {
    let total_gb = profile.total_ram_gb();

    if total_gb < 2.5 {
        QuantType::Q2K
    } else if total_gb < 3.5 {
        QuantType::Q3KS
    } else if total_gb < 4.5 {
        QuantType::Q4KM
    } else if total_gb < 6.0 {
        QuantType::Q5KM
    } else if total_gb < 8.0 {
        QuantType::Q6K
    } else {
        QuantType::Q8_0
    }
}

/// Estimate the maximum model size (in bytes) that can safely fit in RAM.
pub fn max_model_size_bytes(profile: &DeviceProfile, safety_margin: f32) -> u64 {
    let usable = profile.free_ram_bytes as f64 * (1.0 - safety_margin as f64);
    usable as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{CpuArch, GpuType, Platform};

    fn profile_with_ram(total_gb: f64) -> DeviceProfile {
        let total_bytes = (total_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        DeviceProfile {
            total_ram_bytes: total_bytes,
            free_ram_bytes: total_bytes / 2,
            cpu_cores: 4,
            cpu_arch: CpuArch::Aarch64,
            has_gpu: false,
            gpu_type: GpuType::None,
            platform: Platform::Android,
            battery_level: None,
            is_charging: false,
            available_storage_bytes: 32 * 1024 * 1024 * 1024,
            benchmark_tokens_per_sec: None,
        }
    }

    #[test]
    fn test_budget_2gb_device() {
        let profile = profile_with_ram(2.0);
        let quant = recommend_quantization(&profile);
        assert_eq!(quant, QuantType::Q2K);
    }

    #[test]
    fn test_3gb_device() {
        let profile = profile_with_ram(3.0);
        let quant = recommend_quantization(&profile);
        assert_eq!(quant, QuantType::Q3KS);
    }

    #[test]
    fn test_4gb_device() {
        let profile = profile_with_ram(4.0);
        let quant = recommend_quantization(&profile);
        assert_eq!(quant, QuantType::Q4KM);
    }

    #[test]
    fn test_6gb_device() {
        let profile = profile_with_ram(6.0);
        let quant = recommend_quantization(&profile);
        assert_eq!(quant, QuantType::Q6K);
    }

    #[test]
    fn test_8gb_plus_device() {
        let profile = profile_with_ram(8.0);
        let quant = recommend_quantization(&profile);
        assert_eq!(quant, QuantType::Q8_0);
    }
}
