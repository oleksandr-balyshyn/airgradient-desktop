//! Mapping from semantic sensor status to CSS class names.
//!
//! `sensors::thresholds` decides *what* a reading means (`StatusColor::Red`).
//! This module is the presentation boundary that decides *how* that looks, by
//! naming a CSS class defined in `assets/dashboard.css`.
//!
//! Keeping the colors in CSS rather than in Rust means a card's accent, its
//! gradient, and its little status dot are all styled from one place, and the
//! Rust side only ever passes a class name around.

use crate::sensors::thresholds::StatusColor;

/// Class used when a sensor value is missing or outside the classified range.
pub const UNKNOWN_CLASS: &str = "status-unknown";

/// Translate a semantic status into its CSS class.
///
/// Purple and gray exist for the extreme ends of the AQI scale, which has its
/// own dedicated `aqi-*` classes. On a regular sensor card they fall back to the
/// neutral "unknown" styling rather than introducing two more card gradients.
pub const fn css_class(color: StatusColor) -> &'static str {
    match color {
        StatusColor::Green => "status-green",
        StatusColor::Yellow => "status-yellow",
        StatusColor::Orange => "status-orange",
        StatusColor::Red => "status-red",
        StatusColor::Purple | StatusColor::Gray => UNKNOWN_CLASS,
    }
}

/// Classify an optional reading, falling back to "unknown" when absent.
pub fn class_for(value: Option<f32>, classify: impl FnOnce(f32) -> StatusColor) -> &'static str {
    value.map(classify).map_or(UNKNOWN_CLASS, css_class)
}

/// CSS class for the large AQI card, which uses the full six-step US AQI scale.
pub fn aqi_class(value: Option<f32>) -> &'static str {
    match value {
        None => "aqi-good",
        Some(value) if value <= 50.0 => "aqi-good",
        Some(value) if value <= 100.0 => "aqi-moderate",
        Some(value) if value <= 150.0 => "aqi-sensitive",
        Some(value) if value <= 200.0 => "aqi-unhealthy",
        Some(value) if value <= 300.0 => "aqi-very-unhealthy",
        Some(_) => "aqi-hazardous",
    }
}

#[cfg(test)]
mod tests {
    use super::{aqi_class, class_for, UNKNOWN_CLASS};
    use crate::sensors::thresholds::co2_status_color;

    #[test]
    fn missing_value_is_unknown() {
        assert_eq!(class_for(None, co2_status_color), UNKNOWN_CLASS);
    }

    #[test]
    fn co2_classes_follow_thresholds() {
        assert_eq!(class_for(Some(500.0), co2_status_color), "status-green");
        assert_eq!(class_for(Some(1500.0), co2_status_color), "status-orange");
        assert_eq!(class_for(Some(2500.0), co2_status_color), "status-red");
    }

    #[test]
    fn aqi_classes_cover_the_scale() {
        assert_eq!(aqi_class(None), "aqi-good");
        assert_eq!(aqi_class(Some(50.0)), "aqi-good");
        assert_eq!(aqi_class(Some(75.0)), "aqi-moderate");
        assert_eq!(aqi_class(Some(125.0)), "aqi-sensitive");
        assert_eq!(aqi_class(Some(175.0)), "aqi-unhealthy");
        assert_eq!(aqi_class(Some(250.0)), "aqi-very-unhealthy");
        assert_eq!(aqi_class(Some(400.0)), "aqi-hazardous");
    }
}
