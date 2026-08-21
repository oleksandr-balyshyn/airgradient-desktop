//! Trend text and value formatting.
//!
//! Every dashboard card shows how a reading moved since the previous one, for
//! example `↑ +42 ppm`. Working out that string is pure arithmetic, so it lives
//! here instead of inside a widget. That keeps it unit-testable without needing
//! a GTK display connection, and it means every card formats trends the same way.

/// Whether one direction of travel is better for a given reading.
///
/// This replaces a `lower_is_better: bool` argument. Pollutants really are
/// better when they fall, but comfort readings are not: a room going from 22°C
/// to 21°C has not improved, it has just changed, and colouring that fall green
/// states a verdict the measurement does not support. Naming the two cases makes
/// the call sites say which kind of reading they are describing.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Preference {
    /// Lower is better: pollutants, particulates, AQI.
    LowerIsBetter,
    /// Neither direction is better on its own. Comfort readings such as
    /// temperature and humidity are best inside a range, so a movement is
    /// reported without a good-or-bad verdict.
    Neither,
}

/// Which way a reading moved, in terms of whether that is good or bad.
///
/// "Improved" is not the same as "went down": whether a fall is good depends on
/// what is being measured, which is what `Preference` says.
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
        preference: Preference,
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
        let arrow = if rising { "↑" } else { "↓" };
        let sign = if rising { "+" } else { "" };

        Self {
            text: format!("{arrow} {sign}{} {unit}", format_delta(delta)),
            direction: match preference {
                Preference::LowerIsBetter if rising => TrendDirection::Worse,
                Preference::LowerIsBetter => TrendDirection::Improved,
                Preference::Neither => TrendDirection::Neutral,
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

/// A reading's text, or `--` when the device did not report that sensor.
///
/// A missing reading is not zero, so it gets a placeholder rather than a number.
pub fn format_optional_metric_value(value: Option<f32>) -> String {
    value.map_or_else(|| "--".to_string(), format_metric_value)
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
    use super::{
        format_metric_value, format_optional_metric_value, Preference, Trend, TrendDirection,
    };

    #[test]
    fn a_missing_reading_formats_as_a_placeholder() {
        assert_eq!(format_optional_metric_value(None), "--");
        assert_eq!(format_optional_metric_value(Some(13.24)), "13.2");
    }

    #[test]
    fn missing_current_reading_is_neutral() {
        let trend = Trend::between(None, Some(400.0), "ppm", Preference::LowerIsBetter);

        assert_eq!(trend.text, "No reading");
        assert_eq!(trend.direction, TrendDirection::Neutral);
    }

    #[test]
    fn first_reading_has_nothing_to_compare_against() {
        let trend = Trend::between(Some(400.0), None, "ppm", Preference::LowerIsBetter);

        assert_eq!(trend.text, "No previous reading");
        assert_eq!(trend.direction, TrendDirection::Neutral);
    }

    #[test]
    fn tiny_changes_are_reported_as_flat() {
        let trend = Trend::between(Some(400.01), Some(400.0), "ppm", Preference::LowerIsBetter);

        assert_eq!(trend.text, "→ 0 ppm");
        assert_eq!(trend.direction, TrendDirection::Neutral);
    }

    #[test]
    fn falling_pollutant_is_an_improvement() {
        let trend = Trend::between(Some(400.0), Some(442.0), "ppm", Preference::LowerIsBetter);

        assert_eq!(trend.text, "↓ -42 ppm");
        assert_eq!(trend.direction, TrendDirection::Improved);
    }

    #[test]
    fn rising_pollutant_is_worse() {
        let trend = Trend::between(Some(442.0), Some(400.0), "ppm", Preference::LowerIsBetter);

        assert_eq!(trend.text, "↑ +42 ppm");
        assert_eq!(trend.direction, TrendDirection::Worse);
    }

    /// A comfort reading gets an arrow but no verdict.
    ///
    /// Temperature and humidity are best inside a range rather than as low as
    /// possible, so calling a fall an "improvement" -- and colouring it green --
    /// claims something the reading does not say. The movement is still shown.
    #[test]
    fn comfort_readings_move_without_being_judged() {
        let cooling = Trend::between(Some(21.0), Some(22.0), "°C", Preference::Neither);

        assert_eq!(cooling.text, "↓ -1 °C");
        assert_eq!(cooling.direction, TrendDirection::Neutral);

        let warming = Trend::between(Some(23.0), Some(22.0), "°C", Preference::Neither);

        assert_eq!(warming.text, "↑ +1 °C");
        assert_eq!(warming.direction, TrendDirection::Neutral);
    }

    #[test]
    fn small_deltas_keep_one_decimal() {
        let trend = Trend::between(Some(13.2), Some(11.0), "µg/m³", Preference::LowerIsBetter);

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
