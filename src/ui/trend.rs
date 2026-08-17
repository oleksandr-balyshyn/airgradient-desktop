//! Trend text and value formatting.
//!
//! Every dashboard card shows how a reading moved since the previous one, for
//! example `↑ +42 ppm`. Working out that string is pure arithmetic, so it lives
//! here instead of inside a widget. That keeps it unit-testable without needing
//! a GTK display connection, and it means every card formats trends the same way.

/// Which way a reading moved, in terms of whether that is good or bad.
///
/// "Improved" is not the same as "went down". For most pollutants a lower
/// number is better, but for a reading where higher is better the caller passes
/// `lower_is_better: false` and the meaning flips.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TrendDirection {
    Improved,
    Worse,
    Neutral,
}

impl TrendDirection {
    /// CSS class that colors the trend label.
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Improved => "trend-improved",
            Self::Worse => "trend-worse",
            Self::Neutral => "trend-neutral",
        }
    }
}

/// A rendered trend: the label text plus how to color it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Trend {
    pub text: String,
    pub direction: TrendDirection,
}

/// Differences smaller than this are reported as "no change".
///
/// Sensors jitter slightly between readings. Without a dead zone every card
/// would show a meaningless ±0.01 arrow on every refresh.
const FLAT_DELTA: f32 = 0.05;

impl Trend {
    /// Compare a new reading against the previous one.
    ///
    /// Both values are optional because a sensor may be missing from the device
    /// payload, and because the very first reading has nothing to compare against.
    pub fn between(
        current: Option<f32>,
        previous: Option<f32>,
        unit: &str,
        lower_is_better: bool,
    ) -> Self {
        let Some(current) = current else {
            return Self::neutral("No reading");
        };
        let Some(previous) = previous else {
            return Self::neutral("No previous reading");
        };

        let delta = current - previous;
        if delta.abs() < FLAT_DELTA {
            return Self::neutral(format!("→ 0 {unit}"));
        }

        let rising = delta > 0.0;
        let improves = rising != lower_is_better;
        let arrow = if rising { "↑" } else { "↓" };
        let sign = if rising { "+" } else { "" };

        Self {
            text: format!("{arrow} {sign}{} {unit}", format_delta(delta)),
            direction: if improves {
                TrendDirection::Improved
            } else {
                TrendDirection::Worse
            },
        }
    }

    fn neutral(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            direction: TrendDirection::Neutral,
        }
    }
}

impl Default for Trend {
    fn default() -> Self {
        Self::neutral("No previous reading")
    }
}

/// Format a measurement for display.
///
/// Large or whole numbers are shown without decimals (`447`, `24`), while small
/// fractional ones keep one decimal (`13.2`) so precision is not lost where it
/// matters.
pub fn format_metric_value(value: f32) -> String {
    if value.abs() >= 100.0 || value.fract().abs() < FLAT_DELTA {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// Format a delta. Deltas use a wider "no decimals" band than raw values
/// because a change of `+12.3 ppm` reads no better than `+12 ppm`.
fn format_delta(value: f32) -> String {
    if value.abs() >= 10.0 || value.fract().abs() < FLAT_DELTA {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::{format_metric_value, Trend, TrendDirection};

    #[test]
    fn missing_current_reading_is_neutral() {
        let trend = Trend::between(None, Some(400.0), "ppm", true);

        assert_eq!(trend.text, "No reading");
        assert_eq!(trend.direction, TrendDirection::Neutral);
    }

    #[test]
    fn first_reading_has_nothing_to_compare_against() {
        let trend = Trend::between(Some(400.0), None, "ppm", true);

        assert_eq!(trend.text, "No previous reading");
        assert_eq!(trend.direction, TrendDirection::Neutral);
    }

    #[test]
    fn tiny_changes_are_reported_as_flat() {
        let trend = Trend::between(Some(400.01), Some(400.0), "ppm", true);

        assert_eq!(trend.text, "→ 0 ppm");
        assert_eq!(trend.direction, TrendDirection::Neutral);
    }

    #[test]
    fn falling_pollutant_is_an_improvement() {
        let trend = Trend::between(Some(400.0), Some(442.0), "ppm", true);

        assert_eq!(trend.text, "↓ -42 ppm");
        assert_eq!(trend.direction, TrendDirection::Improved);
    }

    #[test]
    fn rising_pollutant_is_worse() {
        let trend = Trend::between(Some(442.0), Some(400.0), "ppm", true);

        assert_eq!(trend.text, "↑ +42 ppm");
        assert_eq!(trend.direction, TrendDirection::Worse);
    }

    #[test]
    fn direction_flips_when_higher_is_better() {
        let trend = Trend::between(Some(442.0), Some(400.0), "ppm", false);

        assert_eq!(trend.direction, TrendDirection::Improved);
    }

    #[test]
    fn small_deltas_keep_one_decimal() {
        let trend = Trend::between(Some(13.2), Some(11.0), "µg/m³", true);

        assert_eq!(trend.text, "↑ +2.2 µg/m³");
    }

    #[test]
    fn values_drop_decimals_when_large_or_whole() {
        assert_eq!(format_metric_value(447.0), "447");
        assert_eq!(format_metric_value(24.0), "24");
        assert_eq!(format_metric_value(13.24), "13.2");
        assert_eq!(format_metric_value(120.5), "120");
    }
}
