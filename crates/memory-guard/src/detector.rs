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

        self.real_available()
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

impl Default for SystemMemoryDetector {
    fn default() -> Self {
        Self::new()
    }
}
