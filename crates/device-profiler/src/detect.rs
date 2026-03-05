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

        let total_ram_bytes = simulated_total_ram().unwrap_or_else(|| {
            platform_total_memory_bytes().unwrap_or_else(|| system.total_memory())
        });

        let free_ram_bytes = if simulated_total_ram().is_some() {
            simulated_free_ram(total_ram_bytes, &system)
        } else {
            platform_available_memory_bytes().unwrap_or_else(|| {
                let avail = system.available_memory();
                if avail > 0 {
                    avail
                } else {
                    system.free_memory()
                }
            })
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

fn platform_total_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "android")]
    {
        return android_meminfo_bytes().map(|(total, _)| total);
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        return apple_total_memory_bytes();
    }

    #[allow(unreachable_code)]
    None
}

fn platform_available_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "android")]
    {
        return android_meminfo_bytes().map(|(_, available)| available);
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        return apple_available_memory_bytes();
    }

    #[allow(unreachable_code)]
    None
}

#[cfg(target_os = "android")]
fn android_meminfo_bytes() -> Option<(u64, u64)> {
    let data = std::fs::read_to_string("/proc/meminfo").ok()?;
    let total_kb = parse_meminfo_kb(&data, "MemTotal")?;
    let available_kb =
        parse_meminfo_kb(&data, "MemAvailable").or_else(|| parse_meminfo_kb(&data, "MemFree"))?;
    Some((total_kb * 1024, available_kb * 1024))
}

#[cfg(target_os = "android")]
fn parse_meminfo_kb(meminfo: &str, key: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let (k, rest) = line.split_once(':')?;
        if k != key {
            return None;
        }
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })
}

/// Total physical RAM via sysctl on macOS/iOS.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn apple_total_memory_bytes() -> Option<u64> {
    let mut memsize: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    let key = b"hw.memsize\0";
    // SAFETY: `key` is NUL-terminated and output buffers are valid for writes.
    let result = unsafe {
        libc::sysctlbyname(
            key.as_ptr().cast(),
            (&mut memsize as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 {
        Some(memsize)
    } else {
        None
    }
}

/// Available memory via `os_proc_available_memory()` on macOS/iOS.
/// Falls back to vm_statistics64 free+inactive+purgeable if unavailable.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn apple_available_memory_bytes() -> Option<u64> {
    extern "C" {
        fn os_proc_available_memory() -> usize;
    }
    // SAFETY: Simple C function with no preconditions (macOS 12+ / iOS 15+).
    let available = unsafe { os_proc_available_memory() };
    if available > 0 {
        return Some(available as u64);
    }

    apple_vm_stat_available()
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
#[allow(deprecated)]
fn apple_vm_stat_available() -> Option<u64> {
    // SAFETY: mach host APIs are called with valid pointers and checked return codes.
    unsafe {
        let host = libc::mach_host_self();
        let page_size = libc::vm_page_size as u64;

        let mut vm_stat: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = (std::mem::size_of::<libc::vm_statistics64_data_t>()
            / std::mem::size_of::<libc::integer_t>())
            as libc::mach_msg_type_number_t;

        let result = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&mut vm_stat as *mut libc::vm_statistics64).cast::<libc::integer_t>(),
            &mut count,
        );
        if result != libc::KERN_SUCCESS {
            return None;
        }

        let available_pages =
            vm_stat.free_count + vm_stat.inactive_count + vm_stat.purgeable_count;
        Some((available_pages as u64) * page_size)
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
