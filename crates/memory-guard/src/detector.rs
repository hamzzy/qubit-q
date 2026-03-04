use sysinfo::{MemoryRefreshKind, RefreshKind, System};

/// Detects system memory using the `sysinfo` crate.
/// Works on macOS, Linux, and Windows without conditional compilation.
pub struct SystemMemoryDetector {
    system: System,
}

impl SystemMemoryDetector {
    pub fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        Self { system }
    }

    /// Refresh memory stats from the OS.
    pub fn refresh(&mut self) {
        self.system.refresh_memory();
    }

    /// Total physical RAM in bytes.
    pub fn total_ram(&mut self) -> u64 {
        self.refresh();
        self.system.total_memory()
    }

    /// Available RAM in bytes.
    /// Uses available_memory when non-zero, otherwise falls back to free_memory.
    /// On some macOS versions, available_memory may report 0.
    pub fn available_ram(&mut self) -> u64 {
        self.refresh();
        let available = self.system.available_memory();
        if available > 0 {
            available
        } else {
            // Fallback: free_memory + estimate reclaimable cache
            self.system.free_memory()
        }
    }

    /// Used RAM in bytes.
    pub fn used_ram(&mut self) -> u64 {
        self.refresh();
        self.system.used_memory()
    }
}

impl Default for SystemMemoryDetector {
    fn default() -> Self {
        Self::new()
    }
}
