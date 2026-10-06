//! A voice-local estimate, updated only from completed synthesis.
#[derive(Debug, Clone)]
pub struct DurationEstimator {
    scale: f64,
}
impl Default for DurationEstimator {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}
impl DurationEstimator {
    pub fn seconds(&self, text: &str) -> f64 {
        baseline(text) * self.scale
    }
    pub fn observe(&mut self, text: &str, seconds: f64) {
        let expected = baseline(text);
        if expected > 0.0 && seconds.is_finite() && seconds > 0.0 {
            self.scale = self.scale * 0.75 + (seconds / expected).clamp(0.25, 4.0) * 0.25;
        }
    }
}
fn baseline(text: &str) -> f64 {
    let cjk = text.chars().filter(|c| matches!(c,'\u{3400}'..='\u{4dbf}'|'\u{4e00}'..='\u{9fff}'|'\u{3040}'..='\u{30ff}'|'\u{ac00}'..='\u{d7af}')).count();
    let mut word = false;
    let mut words = 0;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if !word {
                words += 1;
            }
            word = true;
        } else {
            word = false;
        }
    }
    cjk as f64 / 4.0 + words as f64 / 2.5
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_and_resets() {
        let mut estimate = DurationEstimator::default();
        assert_eq!(estimate.seconds("甲乙丙丁 one two"), 1.8);
        estimate.observe("甲乙丙丁", 2.0);
        assert_eq!(estimate.seconds("甲乙丙丁"), 1.25);
        assert_eq!(DurationEstimator::default().seconds("甲乙丙丁"), 1.0);
    }
}
