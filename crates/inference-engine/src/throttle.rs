use std::time::Duration;

/// Device thermal state used to throttle inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    Normal,
    Fair,
    Serious,
    Critical,
}

/// Thermal throttling policy interface.
pub trait ThermalThrottlePolicy: Send + Sync {
    fn current_state(&self) -> ThermalState;
    fn suggested_delay(&self) -> Duration;
}

/// Phase-2 stub policy: no throttling yet.
#[derive(Debug, Default, Clone)]
pub struct NoopThermalThrottle;

impl ThermalThrottlePolicy for NoopThermalThrottle {
    fn current_state(&self) -> ThermalState {
        ThermalState::Normal
    }

    fn suggested_delay(&self) -> Duration {
        Duration::from_millis(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_throttle_is_normal() {
        let throttle = NoopThermalThrottle;
        assert_eq!(throttle.current_state(), ThermalState::Normal);
        assert_eq!(throttle.suggested_delay(), Duration::from_millis(0));
    }
}
