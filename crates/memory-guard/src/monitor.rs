use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::detector::SystemMemoryDetector;

/// Events emitted by the background memory monitor.
#[derive(Debug, Clone)]
pub enum MemoryEvent {
    /// Memory usage is normal.
    Normal { used_pct: f32 },
    /// Memory usage above warning threshold.
    Warning { used_pct: f32 },
    /// Memory usage critically high — eviction recommended.
    Critical { used_pct: f32 },
}

/// Background task that monitors system memory and emits events.
pub struct MemoryMonitor {
    cancel: CancellationToken,
    handle: JoinHandle<()>,
}

impl MemoryMonitor {
    /// Start the background memory monitor.
    /// Returns the monitor handle and a receiver for memory events.
    pub fn start(
        interval_ms: u64,
        warning_pct: f32,
        critical_pct: f32,
    ) -> (Self, mpsc::Receiver<MemoryEvent>) {
        let (tx, rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            info!(interval_ms, "Memory monitor started");
            let mut detector = SystemMemoryDetector::new();
            let mut last_state = MonitorState::Normal;

            loop {
                tokio::select! {
                    _ = cancel_clone.cancelled() => {
                        info!("Memory monitor stopped");
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)) => {
                        let total = detector.total_ram();
                        let free = detector.available_ram();
                        let used_pct = if total > 0 {
                            (total - free) as f32 / total as f32
                        } else {
                            0.0
                        };

                        let event = if used_pct > critical_pct {
                            MemoryEvent::Critical { used_pct }
                        } else if used_pct > warning_pct {
                            MemoryEvent::Warning { used_pct }
                        } else {
                            MemoryEvent::Normal { used_pct }
                        };

                        // Only log on state transitions
                        let new_state = MonitorState::from(&event);
                        if new_state != last_state {
                            match &event {
                                MemoryEvent::Critical { used_pct } => {
                                    warn!(used_pct = format!("{:.1}%", used_pct * 100.0), "Memory CRITICAL");
                                }
                                MemoryEvent::Warning { used_pct } => {
                                    warn!(used_pct = format!("{:.1}%", used_pct * 100.0), "Memory WARNING");
                                }
                                MemoryEvent::Normal { used_pct } => {
                                    debug!(used_pct = format!("{:.1}%", used_pct * 100.0), "Memory normal");
                                }
                            }
                            last_state = new_state;
                        }

                        // Best-effort send (don't block if receiver is full)
                        let _ = tx.try_send(event);
                    }
                }
            }
        });

        (Self { cancel, handle }, rx)
    }

    /// Stop the memory monitor.
    pub fn stop(&self) {
        let _ = self.handle.is_finished();
        self.cancel.cancel();
    }
}

impl Drop for MemoryMonitor {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.handle.abort();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MonitorState {
    Normal,
    Warning,
    Critical,
}

impl From<&MemoryEvent> for MonitorState {
    fn from(event: &MemoryEvent) -> Self {
        match event {
            MemoryEvent::Normal { .. } => MonitorState::Normal,
            MemoryEvent::Warning { .. } => MonitorState::Warning,
            MemoryEvent::Critical { .. } => MonitorState::Critical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitor_emits_events() {
        let (monitor, mut rx) = MemoryMonitor::start(50, 0.75, 0.90);

        // Should receive at least one event quickly
        let event = tokio::time::timeout(tokio::time::Duration::from_millis(200), rx.recv()).await;

        assert!(event.is_ok());
        let event = event.unwrap();
        assert!(event.is_some());

        monitor.stop();
    }

    #[tokio::test]
    async fn test_monitor_stop() {
        let (monitor, mut rx) = MemoryMonitor::start(50, 0.75, 0.90);
        monitor.stop();

        // After stop, channel should close eventually
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        // Drain remaining
        while rx.try_recv().is_ok() {}
    }
}
