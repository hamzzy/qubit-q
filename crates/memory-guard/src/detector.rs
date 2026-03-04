use sysinfo::{MemoryRefreshKind, RefreshKind, System};
use tracing::info;

/// Detects system memory using the `sysinfo` crate.
/// Supports `SIMULATE_RAM_MB` env var to simulate low-RAM devices for testing.
pub struct SystemMemoryDetector {
    system: System,
    simulated_total: Option<u64>,
}

impl SystemMemoryDetector {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );

        let simulated_total = std::env::var("SIMULATE_RAM_MB")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|mb| {
                let bytes = mb * 1024 * 1024;
                info!(mb, "RAM simulation enabled via SIMULATE_RAM_MB");
                bytes
            });

        Self {
            system,
            simulated_total,
        }
    }

    /// Refresh memory stats from the OS.
    pub fn refresh(&mut self) {
        self.system.refresh_memory();
    }

    /// Total physical RAM in bytes.
    pub fn total_ram(&mut self) -> u64 {
        self.refresh();
        self.simulated_total
            .or_else(platform_total_memory_bytes)
            .unwrap_or_else(|| self.system.total_memory())
    }

    /// Available RAM in bytes.
    pub fn available_ram(&mut self) -> u64 {
        self.refresh();

        if let Some(sim_total) = self.simulated_total {
            // Simulate: apply same usage ratio as real system, with 1GB OS overhead floor
            let real_total = self.system.total_memory();
            if real_total == 0 {
                return sim_total / 2;
            }
            let real_free = self.real_available();
            let used_ratio = 1.0 - (real_free as f64 / real_total as f64);
            let os_overhead = 1024 * 1024 * 1024u64;
            let sim_used = (sim_total as f64 * used_ratio) as u64;
            return sim_total.saturating_sub(sim_used.max(os_overhead));
        }

        platform_available_memory_bytes().unwrap_or_else(|| self.real_available())
    }

    /// Used RAM in bytes.
    pub fn used_ram(&mut self) -> u64 {
        let total = self.total_ram();
        let available = self.available_ram();
        total.saturating_sub(available)
    }

    /// Whether RAM simulation is active.
    pub fn is_simulated(&self) -> bool {
        self.simulated_total.is_some()
    }

    fn real_available(&self) -> u64 {
        let available = self.system.available_memory();
        if available > 0 {
            available
        } else {
            self.system.free_memory()
        }
    }
}

fn platform_total_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "android")]
    {
        return android_meminfo_bytes().map(|(total, _)| total);
    }

    #[cfg(target_os = "ios")]
    {
        return ios_total_memory_bytes();
    }

    #[allow(unreachable_code)]
    None
}

fn platform_available_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "android")]
    {
        return android_meminfo_bytes().map(|(_, available)| available);
    }

    #[cfg(target_os = "ios")]
    {
        return ios_available_memory_bytes();
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

#[cfg(target_os = "ios")]
fn ios_total_memory_bytes() -> Option<u64> {
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

#[cfg(target_os = "ios")]
#[allow(deprecated)]
fn ios_available_memory_bytes() -> Option<u64> {
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

        let available_pages = vm_stat.free_count + vm_stat.inactive_count;
        Some((available_pages as u64) * page_size)
    }
}

impl Default for SystemMemoryDetector {
    fn default() -> Self {
        Self::new()
    }
}
