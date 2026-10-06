use std::time::Duration;

/// Targets are wall-clock seconds; queues and playback positions use audio time.
pub(super) struct Buffering {
    pub(super) waiting: bool,
    pub(super) underruns: u32,
    seconds: f64,
}
impl Default for Buffering {
    fn default() -> Self {
        Self {
            waiting: true,
            underruns: 0,
            seconds: 3.0,
        }
    }
}
impl Buffering {
    pub(super) fn target(&self, speed: f32) -> Duration {
        Duration::from_secs_f64((self.seconds * f64::from(speed)).min(20.0))
    }
    pub(super) fn update(&mut self, empty: bool, buffered: Duration, speed: f32, complete: bool) {
        if !self.waiting && empty && !complete {
            self.waiting = true;
            self.underruns = self.underruns.saturating_add(1);
            self.seconds = (self.seconds + 2.0).min(10.0);
        }
        if self.waiting && (complete || buffered >= self.target(speed)) {
            self.waiting = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_recovery_and_short_eof() {
        let mut buffer = Buffering::default();
        buffer.update(false, Duration::from_secs(2), 1.0, false);
        assert!(buffer.waiting);
        buffer.update(false, Duration::from_secs(3), 1.0, false);
        assert!(!buffer.waiting);
        buffer.update(true, Duration::ZERO, 1.0, false);
        assert!(buffer.waiting);
        assert_eq!(buffer.underruns, 1);
        assert_eq!(buffer.target(1.0), Duration::from_secs(5));
        buffer.update(false, Duration::from_secs(1), 1.0, false);
        assert!(buffer.waiting);
        buffer.update(false, Duration::from_secs(1), 1.0, true);
        assert!(!buffer.waiting);
    }
    #[test]
    fn speed_and_recovery_are_bounded_below_prefetch_budget() {
        let mut buffer = Buffering::default();
        assert_eq!(buffer.target(2.0), Duration::from_secs(6));
        for _ in 0..10 {
            buffer.update(false, Duration::from_secs(20), 4.0, false);
            buffer.update(true, Duration::ZERO, 4.0, false);
        }
        assert_eq!(buffer.target(4.0), Duration::from_secs(20));
        buffer.update(true, Duration::ZERO, 4.0, true);
        assert!(!buffer.waiting);
    }
}
