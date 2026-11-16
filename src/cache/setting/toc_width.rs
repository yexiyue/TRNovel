use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Continuous directory width, stored in hundredths of one percent for stable equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TocWidth(u16);
impl Default for TocWidth {
    fn default() -> Self {
        Self(5000)
    }
}
impl TocWidth {
    pub fn from_percent(percent: f64) -> Self {
        if !percent.is_finite() {
            return Self::default();
        }
        Self((percent.clamp(1.0, 99.0) * 100.0).round() as u16)
    }
    pub fn percent(self) -> f64 {
        f64::from(self.0) / 100.0
    }
    pub fn label(self) -> String {
        if self.0.is_multiple_of(100) {
            format!("{}%", self.0 / 100)
        } else {
            format!("{:.2}%", self.percent())
        }
    }
    pub fn width(self, width: u16) -> u16 {
        ((u32::from(width) * u32::from(self.0) + 5000) / 10000) as u16
    }
    pub fn shift(self, points: i32) -> Self {
        Self(
            i32::from(self.0)
                .saturating_add(points.saturating_mul(100))
                .clamp(100, 9900) as u16,
        )
    }
    pub fn from_columns(columns: u16, available: u16) -> Self {
        Self::from_percent(f64::from(columns) * 100.0 / f64::from(available.max(1)))
    }
}
impl Serialize for TocWidth {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.percent())
    }
}
impl<'de> Deserialize<'de> for TocWidth {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_percent(f64::deserialize(deserializer)?))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentages_are_finite_bounded_and_round_trip_cell_widths() {
        assert_eq!(TocWidth::from_percent(f64::NAN), TocWidth::default());
        assert_eq!(TocWidth::from_percent(0.0).percent(), 1.0);
        assert_eq!(TocWidth::from_percent(200.0).percent(), 99.0);
        for columns in 18..88 {
            assert_eq!(TocWidth::from_columns(columns, 117).width(117), columns);
        }
        assert_eq!(TocWidth::from_percent(99.0).shift(1).percent(), 99.0);
        assert_eq!(TocWidth::from_percent(1.0).shift(-1).percent(), 1.0);
    }
}
