use sysinfo::{Disks, MemoryRefreshKind, RefreshKind, System};
use tracing::info;

use crate::error::ProfilerError;
use crate::profile::{CpuArch, DeviceProfile, GpuType, Platform};

/// Detects hardware capabilities of the current system.
pub struct SystemProfiler;

impl SystemProfiler {
    /// Detect full device profile.
    pub fn detect() -> Result<DeviceProfile, ProfilerError> {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        system.refresh_memory();

        let total_ram_bytes = simulated_total_ram().unwrap_or_else(|| system.total_memory());

        let free_ram_bytes = if simulated_total_ram().is_some() {
            simulated_free_ram(total_ram_bytes, &system)
        } else {
            let avail = system.available_memory();
            if avail > 0 {
                avail
            } else {
                system.free_memory()
            }
        };

        let cpu_cores = system.cpus().len() as u32;
        // sysinfo needs a separate refresh for CPU info
        let cpu_cores = if cpu_cores == 0 {
            let mut sys2 = System::new();
            sys2.refresh_cpu_all();
            sys2.cpus().len() as u32
        } else {
            cpu_cores
        };

        let cpu_arch = detect_cpu_arch();
        let platform = detect_platform();
        let gpu_type = detect_gpu(&platform);
        let available_storage_bytes = detect_available_storage();

        let profile = DeviceProfile {
            total_ram_bytes,
            free_ram_bytes,
            cpu_cores,
            cpu_arch,
            has_gpu: gpu_type != GpuType::None,
            gpu_type,
            platform,
            battery_level: None, // Not available on desktop
            is_charging: false,
            available_storage_bytes,
            benchmark_tokens_per_sec: None,
        };

        info!(
            total_ram_gb = format!("{:.1}", profile.total_ram_gb()),
            free_ram_gb = format!("{:.1}", profile.usable_ram_gb()),
            cpu_cores,
            arch = %cpu_arch,
            platform = %platform,
            gpu = %gpu_type,
            "Device profiled"
        );

        Ok(profile)
    }
}

/// Check if RAM is being simulated via SIMULATE_RAM_MB env var.
fn simulated_total_ram() -> Option<u64> {
    std::env::var("SIMULATE_RAM_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|mb| mb * 1024 * 1024)
}

/// When simulating RAM, compute free RAM proportionally.
fn simulated_free_ram(simulated_total: u64, system: &System) -> u64 {
    let real_total = system.total_memory();
    if real_total == 0 {
        return simulated_total / 2;
    }
    let real_available = {
        let avail = system.available_memory();
        if avail > 0 {
            avail
        } else {
            system.free_memory()
        }
    };
    let used_ratio = 1.0 - (real_available as f64 / real_total as f64);
    // Apply same used ratio to simulated RAM, with a minimum OS overhead of ~1GB
    let os_overhead = 1024 * 1024 * 1024u64; // 1 GB
    let simulated_used = (simulated_total as f64 * used_ratio) as u64;
    simulated_total.saturating_sub(simulated_used.max(os_overhead))
}

fn detect_cpu_arch() -> CpuArch {
    if cfg!(target_arch = "aarch64") {
        CpuArch::Aarch64
    } else if cfg!(target_arch = "x86_64") {
        CpuArch::X86_64
    } else if cfg!(target_arch = "arm") {
        CpuArch::Armv7
    } else {
        CpuArch::Other
    }
}

fn detect_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "ios") {
        Platform::Ios
    } else if cfg!(target_os = "android") {
        Platform::Android
    } else {
        Platform::Linux // Default fallback
    }
}

fn detect_gpu(platform: &Platform) -> GpuType {
    match platform {
        Platform::MacOs | Platform::Ios => GpuType::Metal,
        Platform::Android => GpuType::None, // Conservative default; Vulkan detection deferred
        _ => GpuType::None,
    }
}

fn detect_available_storage() -> u64 {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| {
            let mp = d.mount_point();
            mp == std::path::Path::new("/") || mp == std::path::Path::new("C:\\")
        })
        .map(|d| d.available_space())
        .next()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_profile() {
        let profile = SystemProfiler::detect().unwrap();
        assert!(profile.total_ram_bytes > 0);
        assert!(profile.cpu_cores > 0);
    }

    #[test]
    fn test_simulated_ram() {
        // This test validates the simulation logic without setting env var
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let sim_total = 3 * 1024 * 1024 * 1024u64; // 3 GB
        let sim_free = simulated_free_ram(sim_total, &system);
        assert!(sim_free < sim_total);
        assert!(sim_free > 0);
    }
}
